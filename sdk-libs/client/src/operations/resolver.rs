//! Resolves ring keys from tracking leaves opened under a root the registry holds.
//!
//! A leaf commits to `viewing_hash`, never the viewing key, because an X25519 u can exceed the BN254
//! modulus. The indexer serves the key beside the leaf, and a reader accepts it only once the leaf
//! opens under the root and the key hashes to the leaf's `viewing_hash`. A write the enrollment
//! writer still holds grants nothing and resolves nothing.

use std::collections::HashMap;

use custom_ring_interface::{
    domain,
    storage::StorageError,
    tracking::{viewing_hash, TrackingRegistry},
};
use solana_address::Address;
use spr_client::{AsyncRpc, ClientError};
use spr_hasher::{
    primitives::{hash_bytes, solana_owner_identity},
    HasherError,
};
use spr_indexer_api::Hash;
use spr_interface::ZERO_NULLIFIER_PUBKEY;
use spr_keypair::{KeypairError, NullifierKey, X25519Keypair, X25519Pubkey};
use thiserror::Error;
use zolana_ring_indexer::{
    rpc::MAX_RING_MEMBER_PAGE_LIMIT,
    tracking::{GetTrackingOpeningsRequest, RingTrackingOpening, RingTrackingRecordView},
};
use zolana_ring_keys::{RingAddress, RingKeyError};
use zolana_ring_member_registry::{OpeningCheck, RecordTreeError, TrackingOpening, TrackingRecord};

use crate::{
    indexer::{AsyncRingIndexer, RingIndexerError},
    records::path_of,
    AuditError, SealedNullifierKey,
};

/// The salt stays with the address it came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedRecipient {
    pub nullifier_pk: [u8; 32],
    /// The leaf's, a rotation moves it past the address.
    pub viewing_pubkey: X25519Pubkey,
    /// `None` with no row, off every private list. Zero under the zero key.
    pub list_tag: Option<[u8; 32]>,
}

#[derive(Debug, Error)]
pub enum ResolveError {
    #[error(transparent)]
    Address(#[from] RingKeyError),
    #[error(transparent)]
    Keypair(#[from] KeypairError),
    #[error(transparent)]
    Hasher(#[from] HasherError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Indexer(#[from] RingIndexerError),
    #[error(transparent)]
    Rpc(Box<ClientError>),
    #[error("the tracking opening fails against the root")]
    Opening(#[source] RecordTreeError),
    #[error("the tracking opening is malformed")]
    MalformedOpening,
    #[error("the indexer answered {got} openings for {want} owners")]
    OpeningCount { got: usize, want: usize },
    #[error("the address list salt hashes to another list tag than the tracking leaf")]
    ListTagMismatch,
    #[error("the tracking leaf holds another nullifier key than the address")]
    NullifierKeyMismatch,
    #[error("the served viewing key does not hash to the tracking leaf's viewing hash")]
    ViewingKeyMismatch,
    #[error("the served seal does not hash to the tracking leaf or names another owner")]
    SealMismatch,
    #[error("the tracking registry is not an initialized storage program account")]
    TrackingRegistryMissing,
}

/// `registry` is the one the ring's policy names.
#[must_use]
pub struct RecipientResolver<'a> {
    pub address: &'a RingAddress,
    pub registry: Address,
    pub root: [u8; 32],
}

/// One served opening of `member` under `root`.
#[must_use]
pub struct TrackingLeafCheck<'a> {
    pub view: &'a RingTrackingOpening,
    pub root: &'a [u8; 32],
    pub member: &'a [u8; 32],
}

/// A present leaf under the checked root, the viewing key its `viewing_hash` commits to and the seal its `ct_hash` commits to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrackingLeaf {
    pub record: TrackingRecord,
    /// The Solana owner the leaf's member commits to.
    pub owner: Address,
    pub viewing_u: [u8; 32],
    /// Rotations the leaf has taken, `viewing_hash` binds it.
    pub seq: u64,
    pub seal: SealedNullifierKey,
}

impl RecipientResolver<'_> {
    /// One opening under the frame's root settles presence and every key, a later rotation leaves the payment sealed to the key read.
    pub async fn resolve<C: AsyncRingIndexer + ?Sized>(
        self,
        client: &C,
    ) -> Result<ResolvedRecipient, ResolveError> {
        let member = self
            .address
            .recipient
            .signing_pubkey
            .owner_proof_input_hash()?;
        let response = client
            .get_tracking_openings(GetTrackingOpeningsRequest {
                registry: self.registry.into(),
                members: vec![Hash(member)],
                expected_root: Hash(self.root),
            })
            .await?;
        let [opening] = response.openings.as_slice() else {
            return Err(ResolveError::OpeningCount {
                got: response.openings.len(),
                want: 1,
            });
        };
        self.verify(&member, opening)
    }

    /// An owner with no row resolves to the address keys, only a later registration of the same key spends.
    fn verify(
        &self,
        member: &[u8; 32],
        view: &RingTrackingOpening,
    ) -> Result<ResolvedRecipient, ResolveError> {
        let address = self.address;
        RingAddress::check_list_salt(&address.recipient, &address.list_salt)?;
        let nullifier_pk = address.recipient.nullifier_pubkey;
        let opened = TrackingLeafCheck {
            view,
            root: &self.root,
            member,
        }
        .verify()?;
        let Some(TrackingLeaf {
            record, viewing_u, ..
        }) = opened
        else {
            return Ok(ResolvedRecipient {
                nullifier_pk,
                viewing_pubkey: address.recipient.viewing_pubkey,
                list_tag: None,
            });
        };
        let list_tag = if nullifier_pk == ZERO_NULLIFIER_PUBKEY {
            [0; 32]
        } else if record.nullifier_pk != nullifier_pk {
            return Err(ResolveError::NullifierKeyMismatch);
        } else if domain::list_tag(&address.list_salt)? != record.list_tag {
            return Err(ResolveError::ListTagMismatch);
        } else {
            record.list_tag
        };
        Ok(ResolvedRecipient {
            nullifier_pk,
            viewing_pubkey: X25519Pubkey::from_u(viewing_u)?,
            list_tag: Some(list_tag),
        })
    }
}

impl TrackingLeafCheck<'_> {
    /// `None` for an absent member, its absence proven under the root.
    pub fn verify(self) -> Result<Option<TrackingLeaf>, ResolveError> {
        let view = self.view;
        let path = path_of(&view.path).map_err(|_| ResolveError::MalformedOpening)?;
        let record = tracking_record(&view.record);
        let opening = match view.present {
            true => TrackingOpening::Present { record, path },
            false => TrackingOpening::Absent { low: record, path },
        };
        let Some(record) = OpeningCheck {
            opening: &opening,
            root: self.root,
            member: self.member,
        }
        .verify()
        .map_err(ResolveError::Opening)?
        else {
            return Ok(None);
        };
        // The root authenticates the leaf's hashes alone, each served preimage counts once it hashes to one.
        let served = view
            .record
            .seal
            .as_ref()
            .ok_or(ResolveError::MalformedOpening)?;
        let viewing_u: [u8; 32] = fixed(&served.viewing_u.0)?;
        if viewing_hash(&viewing_u, served.seq)? != record.viewing_hash {
            return Err(ResolveError::ViewingKeyMismatch);
        }
        let seal = SealedNullifierKey {
            eph_pk: X25519Pubkey::from_bytes(fixed(&served.eph_pk.0)?)?,
            ciphertext: fixed(&served.ciphertext.0)?,
        };
        let owner = served.owner.0;
        if hash_bytes(&seal.ciphertext)? != record.ct_hash
            || solana_owner_identity(owner.as_array())? != record.member
        {
            return Err(ResolveError::SealMismatch);
        }
        Ok(Some(TrackingLeaf {
            record,
            owner,
            viewing_u,
            seq: served.seq,
            seal,
        }))
    }
}

impl TrackingLeaf {
    /// The auditor's opening of the seal, accepted only when its secrets derive the leaf's nullifier key and list tag.
    pub fn nullifier_key(&self, auditor: &X25519Keypair) -> Result<NullifierKey, AuditError> {
        let key = self.seal.open(auditor)?;
        let list_tag = domain::list_tag(&self.seal.open_list_salt(auditor)?)?;
        if key.pubkey()? != self.record.nullifier_pk || list_tag != self.record.list_tag {
            return Err(AuditError::NullifierKeyMismatch);
        }
        Ok(key)
    }
}

/// The registered nullifier keys of ring output owners, read for an auditor.
#[must_use]
pub struct RegisteredKeys<'a> {
    /// The ring's pinned tracking registry.
    pub registry: Address,
    /// Solana owners.
    pub owners: &'a [[u8; 32]],
}

impl RegisteredKeys<'_> {
    /// An owner with no leaf under the newest root is absent and its outputs stay unresolved.
    pub async fn read(
        self,
        rpc: &(impl AsyncRpc + ?Sized),
        indexer: &(impl AsyncRingIndexer + ?Sized),
    ) -> Result<HashMap<[u8; 32], [u8; 32]>, ResolveError> {
        let leaves = self.leaves(rpc, indexer).await?;
        Ok(leaves
            .into_iter()
            .map(|(owner, leaf)| (owner, leaf.record.nullifier_pk))
            .collect())
    }

    /// Each present owner's leaf with its authenticated viewing key and seal.
    pub async fn leaves(
        self,
        rpc: &(impl AsyncRpc + ?Sized),
        indexer: &(impl AsyncRingIndexer + ?Sized),
    ) -> Result<HashMap<[u8; 32], TrackingLeaf>, ResolveError> {
        let mut leaves = HashMap::with_capacity(self.owners.len());
        if self.owners.is_empty() {
            return Ok(leaves);
        }
        // 1. The newest root from the registry account, so an indexer cannot pick the root it serves under.
        let root = rpc
            .get_account(self.registry)
            .await
            .map_err(|error| ResolveError::Rpc(Box::new(error)))?
            .and_then(|account| TrackingRegistry::decode(&account.owner, &account.data))
            .ok_or(ResolveError::TrackingRegistryMissing)?
            .newest()
            .hash;
        // 2. One opening per owner, each checked against that root, a present leaf gives the key.
        for owners in self.owners.chunks(MAX_RING_MEMBER_PAGE_LIMIT as usize) {
            let members = owners
                .iter()
                .map(solana_owner_identity)
                .collect::<Result<Vec<_>, _>>()?;
            let response = indexer
                .get_tracking_openings(GetTrackingOpeningsRequest {
                    registry: self.registry.into(),
                    members: members.iter().copied().map(Hash).collect(),
                    expected_root: Hash(root),
                })
                .await?;
            // A short or long answer would pair an opening with another owner's member.
            if response.openings.len() != owners.len() {
                return Err(ResolveError::OpeningCount {
                    got: response.openings.len(),
                    want: owners.len(),
                });
            }
            for ((owner, member), view) in owners.iter().zip(&members).zip(&response.openings) {
                let opened = TrackingLeafCheck {
                    view,
                    root: &root,
                    member,
                }
                .verify()?;
                if let Some(leaf) = opened {
                    leaves.insert(*owner, leaf);
                }
            }
        }
        Ok(leaves)
    }
}

fn fixed<const N: usize>(bytes: &[u8]) -> Result<[u8; N], ResolveError> {
    bytes.try_into().map_err(|_| ResolveError::MalformedOpening)
}

/// The leaf hashes `ct_hash` and `viewing_hash`, never the seal they commit to.
pub fn tracking_record(view: &RingTrackingRecordView) -> TrackingRecord {
    TrackingRecord {
        member: view.member.0,
        next: view.next.0,
        nullifier_pk: view.nullifier_pk.0,
        ct_hash: view.ct_hash.0,
        list_tag: view.list_tag.0,
        viewing_hash: view.viewing_hash.0,
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use custom_ring_interface::{
        storage_instruction::InitializeTrackingIxData, tracking::TrackingEntry, STORAGE_PROGRAM_ID,
    };
    use solana_account::Account;
    use spr_indexer_api::{Base64String, Context};
    use spr_keypair::{
        shielded::{ShieldedAddress, ShieldedKeypair},
        SigningKey, ViewingKey,
    };
    use zolana_ring_indexer::{
        rpc::{RingRecordPathView, StorageRootView},
        tracking::{GetTrackingOpeningsResponse, RingTrackingSealView},
    };
    use zolana_ring_keys::ListSaltHolder;
    use zolana_ring_member_registry::{RecordOpening, RecordTree, TrackingTree, TreePath};

    use super::*;

    const RING: Address = Address::new_from_array([4; 32]);
    const REGISTRY: Address = Address::new_from_array([5; 32]);
    const CIPHERTEXT: [u8; 64] = [7; 64];

    fn wallet(byte: u8) -> ShieldedKeypair {
        ShieldedKeypair::from_keypair(SigningKey::from_ed25519_bytes(&[byte; 32])).unwrap()
    }

    fn enrolled(owner: &ShieldedKeypair) -> RingAddress {
        RingAddress::enrolled(owner, RING).unwrap()
    }

    fn zero_key(owner: &ShieldedKeypair) -> RingAddress {
        RingAddress {
            recipient: ShieldedAddress {
                nullifier_pubkey: ZERO_NULLIFIER_PUBKEY,
                ..owner.shielded_address().unwrap()
            },
            ring_program_id: RING,
            list_salt: [0; 32],
        }
    }

    fn member(owner: &ShieldedKeypair) -> [u8; 32] {
        owner.signing_pubkey().owner_proof_input_hash().unwrap()
    }

    /// The owner's proven entry holding `viewing`.
    fn entry(owner: &ShieldedKeypair, viewing: X25519Pubkey) -> TrackingEntry {
        TrackingEntry {
            member: member(owner),
            nullifier_pk: owner.nullifier_key.pubkey().unwrap(),
            ct_hash: hash_bytes(&CIPHERTEXT).unwrap(),
            list_tag: domain::list_tag(&owner.list_salt(&RING).unwrap()).unwrap(),
            viewing_hash: viewing_hash(&viewing.u(), 0).unwrap(),
            prior_viewing_hash: [0; 32],
            proven: true,
        }
    }

    fn view_path(path: &impl TreePath) -> RingRecordPathView {
        RingRecordPathView {
            index: path.index(),
            siblings: path
                .siblings()
                .iter()
                .map(|level| level.map(Hash))
                .collect(),
        }
    }

    /// The registry account holding `root` as its newest.
    fn registry_account(root: [u8; 32]) -> Account {
        let mut registry = TrackingRegistry::new(
            Address::default(),
            &InitializeTrackingIxData {
                ring_id: [1; 32],
                auditor_pubkey: [2; 33],
            },
        );
        registry.roots[0].hash = root;
        Account {
            lamports: 1,
            data: bytemuck::bytes_of(&registry).to_vec(),
            owner: STORAGE_PROGRAM_ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    /// The tree an indexer opens and the viewing key it serves beside each leaf.
    struct Served {
        tree: TrackingTree,
        /// Member to the owner and viewing key the seal names.
        keys: HashMap<[u8; 32], (Address, [u8; 32])>,
        seq: u64,
        /// Openings answered per member.
        openings: usize,
        registry: Option<Account>,
        requests: std::sync::Mutex<Vec<usize>>,
    }

    impl Served {
        fn new(enrolled: &[(&ShieldedKeypair, X25519Pubkey)]) -> Self {
            let entries: Vec<_> = enrolled
                .iter()
                .map(|(owner, viewing)| entry(owner, *viewing))
                .collect();
            let keys = enrolled
                .iter()
                .map(|(owner, viewing)| {
                    (
                        member(owner),
                        (Address::new_from_array(solana_owner(owner)), viewing.u()),
                    )
                })
                .collect();
            Self::of(&entries, keys)
        }

        fn of(entries: &[TrackingEntry], keys: HashMap<[u8; 32], (Address, [u8; 32])>) -> Self {
            let tree = TrackingTree::new()
                .unwrap()
                .apply([0; 32], entries)
                .unwrap()
                .tree;
            let registry = Some(registry_account(tree.root().unwrap()));
            Self {
                tree,
                keys,
                seq: 0,
                openings: 1,
                registry,
                requests: std::sync::Mutex::default(),
            }
        }

        fn view_record(&self, record: &TrackingRecord) -> RingTrackingRecordView {
            let seal =
                self.keys
                    .get(&record.member)
                    .map(|(owner, viewing_u)| RingTrackingSealView {
                        owner: (*owner).into(),
                        viewing_u: Base64String(viewing_u.to_vec()),
                        seq: self.seq,
                        eph_pk: Base64String(ViewingKey::new().pubkey().as_bytes().to_vec()),
                        ciphertext: Base64String(CIPHERTEXT.to_vec()),
                    });
            RingTrackingRecordView {
                member: Hash(record.member),
                next: Hash(record.next),
                nullifier_pk: Hash(record.nullifier_pk),
                ct_hash: Hash(record.ct_hash),
                list_tag: Hash(record.list_tag),
                viewing_hash: Hash(record.viewing_hash),
                seal,
            }
        }

        fn view(&self, member: &[u8; 32]) -> RingTrackingOpening {
            let (present, record, path) = match self.tree.opening(member).unwrap() {
                RecordOpening::Present { record, path } => (true, record, path),
                RecordOpening::Absent { low, path } => (false, low, path),
            };
            RingTrackingOpening {
                present,
                record: self.view_record(&record),
                path: view_path(&path),
            }
        }

        async fn resolve(&self, address: &RingAddress) -> Result<ResolvedRecipient, ResolveError> {
            RecipientResolver {
                address,
                registry: REGISTRY,
                root: self.tree.root().unwrap(),
            }
            .resolve(self)
            .await
        }

        async fn keys(
            &self,
            owners: &[[u8; 32]],
        ) -> Result<HashMap<[u8; 32], [u8; 32]>, ResolveError> {
            RegisteredKeys {
                registry: REGISTRY,
                owners,
            }
            .read(self, self)
            .await
        }
    }

    #[async_trait]
    impl AsyncRingIndexer for Served {
        async fn get_tracking_openings(
            &self,
            request: GetTrackingOpeningsRequest,
        ) -> Result<GetTrackingOpeningsResponse, RingIndexerError> {
            // A lying indexer serves its own tree under any requested root.
            let root = self.tree.root().unwrap();
            assert_eq!(request.registry, REGISTRY.into());
            assert!(request.members.len() as u64 <= MAX_RING_MEMBER_PAGE_LIMIT);
            self.requests.lock().unwrap().push(request.members.len());
            Ok(GetTrackingOpeningsResponse {
                context: Context {
                    slot: 1,
                    block_time: 0,
                },
                root: StorageRootView {
                    root: Hash(root),
                    revision: 1,
                    next_index: 2,
                },
                openings: request
                    .members
                    .iter()
                    .flat_map(|member| vec![self.view(&member.0); self.openings])
                    .collect(),
            })
        }
    }

    #[async_trait]
    impl AsyncRpc for Served {
        async fn get_account(&self, address: Address) -> Result<Option<Account>, ClientError> {
            assert_eq!(address, REGISTRY);
            Ok(self.registry.clone())
        }
    }

    fn solana_owner(owner: &ShieldedKeypair) -> [u8; 32] {
        owner.signing_pubkey().confidential_view_tag().unwrap()
    }

    #[tokio::test]
    async fn an_applied_enrollment_resolves_to_the_leaf_viewing_key() {
        let owner = wallet(3);
        let rotated = ViewingKey::new().pubkey();
        let leaf = entry(&owner, rotated);
        assert_eq!(
            Served::new(&[(&owner, rotated)])
                .resolve(&enrolled(&owner))
                .await
                .unwrap(),
            ResolvedRecipient {
                nullifier_pk: leaf.nullifier_pk,
                viewing_pubkey: rotated,
                list_tag: Some(leaf.list_tag),
            }
        );
    }

    /// The leaf binds the hash alone.
    #[tokio::test]
    async fn a_served_viewing_key_off_the_leaf_hash_or_missing_is_refused() {
        let owner = wallet(3);
        let mut served = Served::new(&[(&owner, ViewingKey::new().pubkey())]);
        served.seq = 1;
        assert!(matches!(
            served.resolve(&enrolled(&owner)).await,
            Err(ResolveError::ViewingKeyMismatch)
        ));
        served.seq = 0;
        let solana = Address::new_from_array(solana_owner(&owner));
        served
            .keys
            .insert(member(&owner), (solana, ViewingKey::new().pubkey().u()));
        assert!(matches!(
            served.resolve(&enrolled(&owner)).await,
            Err(ResolveError::ViewingKeyMismatch)
        ));
        served.keys.clear();
        assert!(matches!(
            served.resolve(&enrolled(&owner)).await,
            Err(ResolveError::MalformedOpening)
        ));
    }

    #[tokio::test]
    async fn a_salt_of_another_list_tag_is_refused() {
        let owner = wallet(3);
        let mut address = enrolled(&owner);
        address.list_salt = [9; 32];
        assert!(matches!(
            Served::new(&[(&owner, ViewingKey::new().pubkey())])
                .resolve(&address)
                .await,
            Err(ResolveError::ListTagMismatch)
        ));
    }

    #[tokio::test]
    async fn a_leaf_key_off_the_address_key_is_refused() {
        let owner = wallet(3);
        let viewing = ViewingKey::new().pubkey();
        let mut leaf = entry(&owner, viewing);
        leaf.nullifier_pk = [8; 32];
        let solana = Address::new_from_array(solana_owner(&owner));
        let served = Served::of(
            &[leaf],
            HashMap::from([(leaf.member, (solana, viewing.u()))]),
        );
        assert!(matches!(
            served.resolve(&enrolled(&owner)).await,
            Err(ResolveError::NullifierKeyMismatch)
        ));
    }

    /// The absence opening still verifies, a forged absence never reaches the address keys.
    #[tokio::test]
    async fn a_keyed_address_with_no_row_resolves_to_its_own_keys_off_every_list() {
        let owner = wallet(3);
        let address = enrolled(&owner);
        assert_eq!(
            Served::new(&[(&wallet(4), ViewingKey::new().pubkey())])
                .resolve(&address)
                .await
                .unwrap(),
            ResolvedRecipient {
                nullifier_pk: address.recipient.nullifier_pubkey,
                viewing_pubkey: address.recipient.viewing_pubkey,
                list_tag: None,
            }
        );
    }

    /// Payment validation refuses an enrolled recipient under the zero key.
    #[tokio::test]
    async fn a_zero_key_address_keeps_the_zero_key_and_an_enrolled_owner_takes_the_leaf_viewing_key(
    ) {
        let owner = wallet(3);
        let address = zero_key(&owner);
        assert_eq!(
            Served::new(&[]).resolve(&address).await.unwrap(),
            ResolvedRecipient {
                nullifier_pk: ZERO_NULLIFIER_PUBKEY,
                viewing_pubkey: address.recipient.viewing_pubkey,
                list_tag: None,
            }
        );
        let rotated = ViewingKey::new().pubkey();
        assert_eq!(
            Served::new(&[(&owner, rotated)])
                .resolve(&address)
                .await
                .unwrap(),
            ResolvedRecipient {
                nullifier_pk: ZERO_NULLIFIER_PUBKEY,
                viewing_pubkey: rotated,
                list_tag: Some([0; 32]),
            }
        );
    }

    #[test]
    fn an_opening_under_another_root_and_a_forged_absence_are_refused() {
        let owner = wallet(3);
        let member = member(&owner);
        let enrolled_tree = Served::new(&[(&owner, ViewingKey::new().pubkey())]);
        let check = |root: [u8; 32], view: &RingTrackingOpening| {
            TrackingLeafCheck {
                view,
                root: &root,
                member: &member,
            }
            .verify()
        };
        let honest = enrolled_tree.view(&member);
        assert!(matches!(
            check([1; 32], &honest),
            Err(ResolveError::Opening(RecordTreeError::RootMismatch))
        ));
        let forged = Served::new(&[]).view(&member);
        assert!(matches!(
            check(enrolled_tree.tree.root().unwrap(), &forged),
            Err(ResolveError::Opening(RecordTreeError::RootMismatch))
        ));
    }

    #[tokio::test]
    async fn an_extra_opening_is_refused() {
        let owner = wallet(3);
        let mut served = Served::new(&[(&owner, ViewingKey::new().pubkey())]);
        served.openings = 2;
        assert!(matches!(
            served.resolve(&enrolled(&owner)).await,
            Err(ResolveError::OpeningCount { got: 2, want: 1 })
        ));
        assert!(matches!(
            served.keys(&[solana_owner(&owner)]).await,
            Err(ResolveError::OpeningCount { got: 2, want: 1 })
        ));
    }

    /// An owner with no leaf stays out of the keys, its outputs remain unresolved.
    #[tokio::test]
    async fn registered_keys_take_present_leaves_under_the_registry_root() {
        let owner = wallet(3);
        let absent = wallet(4);
        let served = Served::new(&[(&owner, ViewingKey::new().pubkey())]);
        assert_eq!(
            served
                .keys(&[solana_owner(&owner), solana_owner(&absent)])
                .await
                .unwrap(),
            HashMap::from([(solana_owner(&owner), owner.nullifier_key.pubkey().unwrap())])
        );
        let mut stale = Served::new(&[(&owner, ViewingKey::new().pubkey())]);
        stale.registry = Some(registry_account([1; 32]));
        assert!(matches!(
            stale.keys(&[solana_owner(&owner)]).await,
            Err(ResolveError::Opening(RecordTreeError::RootMismatch))
        ));
        stale.registry = None;
        assert!(matches!(
            stale.keys(&[solana_owner(&owner)]).await,
            Err(ResolveError::TrackingRegistryMissing)
        ));
        assert!(stale.keys(&[]).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn registered_keys_read_more_owners_than_one_page_takes() {
        let owners: Vec<[u8; 32]> = (0..2 * MAX_RING_MEMBER_PAGE_LIMIT + 5)
            .map(|index| {
                let mut owner = [0; 32];
                owner[..8].copy_from_slice(&index.to_le_bytes());
                owner
            })
            .collect();
        let served = Served::new(&[]);
        assert!(served.keys(&owners).await.unwrap().is_empty());
        let page = MAX_RING_MEMBER_PAGE_LIMIT as usize;
        assert_eq!(*served.requests.lock().unwrap(), [page, page, 5]);
    }
}
