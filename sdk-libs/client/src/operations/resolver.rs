use std::collections::HashMap;

use custom_ring_interface::{domain, tracking::TrackingAuthorization, STORAGE_PROGRAM_ID};
use solana_account::Account;
use solana_address::Address;
use spr_client::{AsyncRpc, ClientError};
use spr_hasher::HasherError;
use spr_indexer_api::Hash;
use spr_interface::ZERO_NULLIFIER_PUBKEY;
use spr_keypair::{KeypairError, X25519Pubkey};
use thiserror::Error;
use zolana_ring_indexer::tracking::{
    GetTrackingOpeningsRequest, RingTrackingOpening, RingTrackingRecordView,
};
use zolana_ring_keys::{RingAddress, RingKeyError};
use zolana_ring_member_registry::{OpeningCheck, RecordTreeError, TrackingOpening, TrackingRecord};

use crate::{
    indexer::{AsyncRingIndexer, RingIndexerError},
    records::path_of,
};

/// The salt stays with the address it came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResolvedRecipient {
    pub nullifier_pk: [u8; 32],
    /// The authorization's, a rotation moves it past the address.
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
    Indexer(#[from] RingIndexerError),
    #[error(transparent)]
    Rpc(Box<ClientError>),
    #[error("the tracking opening fails against the root")]
    Opening(#[source] RecordTreeError),
    #[error("the tracking opening is malformed")]
    MalformedOpening,
    #[error("the indexer answered {0} openings for one owner")]
    OpeningCount(usize),
    #[error("the address list salt hashes to another list tag than the tracking leaf")]
    ListTagMismatch,
    #[error("the tracking leaf holds another nullifier key than the address")]
    NullifierKeyMismatch,
    #[error("the owner's tracking authorization is absent")]
    AuthorizationMissing,
    #[error("the tracking authorization is not a storage program account")]
    AuthorizationProgramOwner,
    #[error("the tracking authorization data is malformed")]
    AuthorizationMalformed,
    #[error("the tracking authorization names another registry")]
    AuthorizationRegistry,
    #[error("the tracking authorization names another owner")]
    AuthorizationOwner,
}

/// `registry` is the one the ring's policy names.
#[must_use]
pub struct RecipientResolver<'a> {
    pub address: &'a RingAddress,
    pub registry: Address,
    pub root: [u8; 32],
}

/// One owner's authorization as an RPC served it, the PDA alone binds no served account.
#[must_use]
pub struct AuthorizationCheck<'a> {
    pub registry: Address,
    pub owner: Address,
    pub account: Option<&'a Account>,
}

/// What the opening alone settles, an enrolled owner still needs its authorization's viewing key.
#[derive(Debug)]
enum Opened {
    Resolved(ResolvedRecipient),
    Enrolled {
        nullifier_pk: [u8; 32],
        list_tag: [u8; 32],
    },
}

impl RecipientResolver<'_> {
    /// Reads the authorization at confirmed commitment, a later rotation leaves the payment sealed to the key read.
    pub async fn resolve<C: AsyncRpc + AsyncRingIndexer + ?Sized>(
        self,
        client: &C,
    ) -> Result<ResolvedRecipient, ResolveError> {
        // 1. The opening under the frame's root settles presence, the nullifier key and the list tag.
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
            return Err(ResolveError::OpeningCount(response.openings.len()));
        };
        let (nullifier_pk, list_tag) = match self.verify(&member, opening)? {
            Opened::Resolved(resolved) => return Ok(resolved),
            Opened::Enrolled {
                nullifier_pk,
                list_tag,
            } => (nullifier_pk, list_tag),
        };
        // 2. The leaf holds no viewing key, the owner's authorization account names it at either key.
        let owner = self.owner()?;
        let (authorization, _) = TrackingAuthorization::find_address(&self.registry, &owner);
        let account = client
            .get_account(authorization)
            .await
            .map_err(|error| ResolveError::Rpc(Box::new(error)))?;
        let viewing_u = AuthorizationCheck {
            registry: self.registry,
            owner,
            account: account.as_ref(),
        }
        .viewing_u()?;
        Ok(ResolvedRecipient {
            nullifier_pk,
            viewing_pubkey: X25519Pubkey::from_u(viewing_u)?,
            list_tag: Some(list_tag),
        })
    }

    /// An owner with no row resolves to the address keys, only a later registration of the same key spends.
    fn verify(
        &self,
        member: &[u8; 32],
        view: &RingTrackingOpening,
    ) -> Result<Opened, ResolveError> {
        let address = self.address;
        RingAddress::check_list_salt(&address.recipient, &address.list_salt)?;
        let path = path_of(&view.path).map_err(|_| ResolveError::MalformedOpening)?;
        let record = tracking_record(&view.record);
        let opening = match view.present {
            true => TrackingOpening::Present { record, path },
            false => TrackingOpening::Absent { low: record, path },
        };
        let row = OpeningCheck {
            opening: &opening,
            root: &self.root,
            member,
        }
        .verify()
        .map_err(ResolveError::Opening)?;
        let nullifier_pk = address.recipient.nullifier_pubkey;
        match (row, nullifier_pk == ZERO_NULLIFIER_PUBKEY) {
            (None, _) => Ok(Opened::Resolved(ResolvedRecipient {
                nullifier_pk,
                viewing_pubkey: address.recipient.viewing_pubkey,
                list_tag: None,
            })),
            (Some(_), true) => Ok(Opened::Enrolled {
                nullifier_pk,
                list_tag: [0; 32],
            }),
            (Some(row), false) if row.nullifier_pk != nullifier_pk => {
                Err(ResolveError::NullifierKeyMismatch)
            }
            (Some(row), false) if domain::list_tag(&address.list_salt)? != row.list_tag => {
                Err(ResolveError::ListTagMismatch)
            }
            (Some(row), false) => Ok(Opened::Enrolled {
                nullifier_pk,
                list_tag: row.list_tag,
            }),
        }
    }

    /// The Solana owner whose identity the tracking member commits.
    fn owner(&self) -> Result<Address, ResolveError> {
        Ok(Address::new_from_array(
            self.address
                .recipient
                .signing_pubkey
                .confidential_view_tag()?,
        ))
    }
}

impl AuthorizationCheck<'_> {
    /// The viewing key the owner last signed into its authorization.
    pub fn viewing_u(self) -> Result<[u8; 32], ResolveError> {
        Ok(self.authorization()?.viewing_u)
    }

    /// Final from the account's creation, an account not yet applied still names it.
    pub fn nullifier_pk(self) -> Result<[u8; 32], ResolveError> {
        Ok(self.authorization()?.nullifier_pk)
    }

    fn authorization(self) -> Result<TrackingAuthorization, ResolveError> {
        let account = self.account.ok_or(ResolveError::AuthorizationMissing)?;
        if account.owner != STORAGE_PROGRAM_ID {
            return Err(ResolveError::AuthorizationProgramOwner);
        }
        let authorization = TrackingAuthorization::from_data(&account.data)
            .ok_or(ResolveError::AuthorizationMalformed)?;
        if authorization.registry != self.registry {
            return Err(ResolveError::AuthorizationRegistry);
        }
        if authorization.owner != self.owner {
            return Err(ResolveError::AuthorizationOwner);
        }
        Ok(*authorization)
    }
}

/// The registered nullifier keys of ring output owners, read for an auditor.
#[must_use]
pub struct RegisteredKeys<'a> {
    /// The ring's pinned tracking registry.
    pub registry: Address,
    pub owners: &'a [[u8; 32]],
}

impl RegisteredKeys<'_> {
    /// An owner with no account is absent and its outputs stay unresolved.
    pub async fn read(
        self,
        rpc: &(impl AsyncRpc + ?Sized),
    ) -> Result<HashMap<[u8; 32], [u8; 32]>, ResolveError> {
        if self.owners.is_empty() {
            return Ok(HashMap::new());
        }
        let owners: Vec<Address> = self
            .owners
            .iter()
            .copied()
            .map(Address::new_from_array)
            .collect();
        let addresses = owners
            .iter()
            .map(|owner| TrackingAuthorization::find_address(&self.registry, owner).0)
            .collect();
        let accounts = rpc
            .get_multiple_accounts(addresses)
            .await
            .map_err(|error| ResolveError::Rpc(Box::new(error)))?;
        let mut keys = HashMap::with_capacity(owners.len());
        for (index, owner) in owners.into_iter().enumerate() {
            let Some(account) = accounts.get(index).and_then(Option::as_ref) else {
                continue;
            };
            let nullifier_pk = AuthorizationCheck {
                registry: self.registry,
                owner,
                account: Some(account),
            }
            .nullifier_pk()?;
            keys.insert(owner.to_bytes(), nullifier_pk);
        }
        Ok(keys)
    }
}

/// The leaf hashes only `ct_hash`.
pub fn tracking_record(view: &RingTrackingRecordView) -> TrackingRecord {
    TrackingRecord {
        member: view.member.0,
        next: view.next.0,
        nullifier_pk: view.nullifier_pk.0,
        ct_hash: view.ct_hash.0,
        list_tag: view.list_tag.0,
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use custom_ring_interface::{
        storage_instruction::EnrollIxData,
        tracking::{FirstEnrollment, TrackingEntry},
    };
    use spr_indexer_api::Context;
    use spr_keypair::{
        shielded::{ShieldedAddress, ShieldedKeypair},
        SigningKey, ViewingKey,
    };
    use zolana_ring_indexer::{
        rpc::{RingRecordPathView, StorageRootView},
        tracking::{GetTrackingOpeningsResponse, RingTrackingOpening},
    };
    use zolana_ring_keys::ListSaltHolder;
    use zolana_ring_member_registry::{RecordOpening, RecordTree, TrackingPath, TrackingTree};

    use super::*;

    const RING: Address = Address::new_from_array([4; 32]);
    const REGISTRY: Address = Address::new_from_array([5; 32]);

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

    fn solana_owner(owner: &ShieldedKeypair) -> Address {
        Address::new_from_array(owner.signing_pubkey().confidential_view_tag().unwrap())
    }

    fn entry(owner: &ShieldedKeypair) -> TrackingEntry {
        TrackingEntry {
            member: member(owner),
            nullifier_pk: owner.nullifier_key.pubkey().unwrap(),
            ct_hash: [7; 32],
            list_tag: domain::list_tag(&owner.list_salt(&RING).unwrap()).unwrap(),
        }
    }

    /// The owner's applied authorization under `registry`, holding `viewing`.
    fn authorization(registry: Address, owner: Address, viewing: X25519Pubkey) -> Account {
        let ix = EnrollIxData {
            nullifier_pk: [0; 32],
            list_tag: [0; 32],
            viewing_u: viewing.u(),
            eph_u: [0; 32],
            ciphertext: [0; 64],
        };
        let mut held = FirstEnrollment {
            registry,
            owner,
            ix: &ix,
        }
        .account();
        held.applied = 1;
        Account {
            lamports: 1,
            data: bytemuck::bytes_of(&held).to_vec(),
            owner: STORAGE_PROGRAM_ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    fn view_path(path: &TrackingPath) -> RingRecordPathView {
        RingRecordPathView {
            index: path.index,
            siblings: path.siblings.iter().map(|level| level.map(Hash)).collect(),
        }
    }

    fn view_record(record: &TrackingRecord) -> RingTrackingRecordView {
        RingTrackingRecordView {
            member: Hash(record.member),
            next: Hash(record.next),
            nullifier_pk: Hash(record.nullifier_pk),
            ct_hash: Hash(record.ct_hash),
            list_tag: Hash(record.list_tag),
            seal: None,
        }
    }

    fn view(tree: &TrackingTree, member: &[u8; 32]) -> RingTrackingOpening {
        match tree.opening(member).unwrap() {
            RecordOpening::Present { record, path } => RingTrackingOpening {
                present: true,
                record: view_record(&record),
                path: view_path(&path),
            },
            RecordOpening::Absent { low, path } => RingTrackingOpening {
                present: false,
                record: view_record(&low),
                path: view_path(&path),
            },
        }
    }

    fn tree(entries: &[TrackingEntry]) -> TrackingTree {
        TrackingTree::new()
            .unwrap()
            .apply([0; 32], entries)
            .unwrap()
            .tree
    }

    /// One opening per owner and the authorization the chain holds, `None` for an absent account.
    struct Served {
        tree: TrackingTree,
        account: Option<Account>,
        openings: usize,
    }

    impl Served {
        fn new(entries: &[TrackingEntry], account: Option<Account>) -> Self {
            Self {
                tree: tree(entries),
                account,
                openings: 1,
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
    }

    #[async_trait]
    impl AsyncRingIndexer for Served {
        async fn get_tracking_openings(
            &self,
            request: GetTrackingOpeningsRequest,
        ) -> Result<GetTrackingOpeningsResponse, RingIndexerError> {
            let root = self.tree.root().unwrap();
            assert_eq!(request.registry, REGISTRY.into());
            assert_eq!(request.expected_root, Hash(root));
            let [member] = request.members.as_slice() else {
                panic!("one member");
            };
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
                openings: vec![view(&self.tree, &member.0); self.openings],
            })
        }
    }

    #[async_trait]
    impl AsyncRpc for Served {
        async fn get_account(&self, _address: Address) -> Result<Option<Account>, ClientError> {
            Ok(self.account.clone())
        }

        async fn get_multiple_accounts(
            &self,
            addresses: Vec<Address>,
        ) -> Result<Vec<Option<Account>>, ClientError> {
            Ok(vec![self.account.clone(); addresses.len()])
        }
    }

    #[tokio::test]
    async fn an_applied_enrollment_resolves_to_the_authorization_viewing_key() {
        let owner = wallet(3);
        let rotated = ViewingKey::new().pubkey();
        let leaf = entry(&owner);
        let served = Served::new(
            &[leaf],
            Some(authorization(REGISTRY, solana_owner(&owner), rotated)),
        );
        assert_eq!(
            served.resolve(&enrolled(&owner)).await.unwrap(),
            ResolvedRecipient {
                nullifier_pk: leaf.nullifier_pk,
                viewing_pubkey: rotated,
                list_tag: Some(leaf.list_tag),
            }
        );
    }

    #[tokio::test]
    async fn an_authorization_off_the_program_registry_or_owner_is_refused() {
        let owner = wallet(3);
        let viewing = ViewingKey::new().pubkey();
        let address = enrolled(&owner);
        let refusal = |account: Option<Account>| async {
            Served::new(&[entry(&owner)], account)
                .resolve(&address)
                .await
                .unwrap_err()
        };
        assert!(matches!(
            refusal(None).await,
            ResolveError::AuthorizationMissing
        ));
        let mut foreign = authorization(REGISTRY, solana_owner(&owner), viewing);
        foreign.owner = Address::new_from_array([6; 32]);
        assert!(matches!(
            refusal(Some(foreign)).await,
            ResolveError::AuthorizationProgramOwner
        ));
        let other_registry = authorization(RING, solana_owner(&owner), viewing);
        assert!(matches!(
            refusal(Some(other_registry)).await,
            ResolveError::AuthorizationRegistry
        ));
        let other_owner = authorization(REGISTRY, solana_owner(&wallet(4)), viewing);
        assert!(matches!(
            refusal(Some(other_owner)).await,
            ResolveError::AuthorizationOwner
        ));
    }

    /// An unapplied authorization already names the key, a missing one leaves the owner unresolved.
    #[tokio::test]
    async fn registered_keys_read_an_authorization_and_refuse_a_foreign_one() {
        let owner = wallet(3);
        let owner_key = solana_owner(&owner).to_bytes();
        let read = |account: Option<Account>| async move {
            RegisteredKeys {
                registry: REGISTRY,
                owners: &[owner_key],
            }
            .read(&Served::new(&[], account))
            .await
        };
        let mut held = authorization(REGISTRY, solana_owner(&owner), ViewingKey::new().pubkey());
        let mut stored = *TrackingAuthorization::from_data(&held.data).unwrap();
        stored.nullifier_pk = [8; 32];
        stored.applied = 0;
        held.data = bytemuck::bytes_of(&stored).to_vec();
        assert_eq!(
            read(Some(held)).await.unwrap(),
            HashMap::from([(owner_key, [8; 32])])
        );
        assert!(read(None).await.unwrap().is_empty());
        let viewing = ViewingKey::new().pubkey();
        let other_registry = authorization(RING, solana_owner(&owner), viewing);
        assert!(matches!(
            read(Some(other_registry)).await,
            Err(ResolveError::AuthorizationRegistry)
        ));
        let other_owner = authorization(REGISTRY, solana_owner(&wallet(4)), viewing);
        assert!(matches!(
            read(Some(other_owner)).await,
            Err(ResolveError::AuthorizationOwner)
        ));
        let mut foreign = authorization(REGISTRY, solana_owner(&owner), viewing);
        foreign.owner = Address::new_from_array([6; 32]);
        assert!(matches!(
            read(Some(foreign)).await,
            Err(ResolveError::AuthorizationProgramOwner)
        ));
    }

    #[tokio::test]
    async fn a_salt_of_another_list_tag_is_refused() {
        let owner = wallet(3);
        let mut address = enrolled(&owner);
        address.list_salt = [9; 32];
        assert!(matches!(
            Served::new(&[entry(&owner)], None).resolve(&address).await,
            Err(ResolveError::ListTagMismatch)
        ));
    }

    #[tokio::test]
    async fn a_leaf_key_off_the_address_key_is_refused() {
        let owner = wallet(3);
        let mut leaf = entry(&owner);
        leaf.nullifier_pk = [8; 32];
        assert!(matches!(
            Served::new(&[leaf], None).resolve(&enrolled(&owner)).await,
            Err(ResolveError::NullifierKeyMismatch)
        ));
    }

    /// The absence opening still verifies, a forged absence never reaches the address keys.
    #[tokio::test]
    async fn a_keyed_address_with_no_row_resolves_to_its_own_keys_off_every_list() {
        let owner = wallet(3);
        let address = enrolled(&owner);
        assert_eq!(
            Served::new(&[entry(&wallet(4))], None)
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
    async fn a_zero_key_address_keeps_the_zero_key_and_an_enrolled_owner_takes_its_current_viewing_key(
    ) {
        let owner = wallet(3);
        let address = zero_key(&owner);
        let unenrolled = Served::new(&[], None).resolve(&address).await.unwrap();
        assert_eq!(
            unenrolled,
            ResolvedRecipient {
                nullifier_pk: ZERO_NULLIFIER_PUBKEY,
                viewing_pubkey: address.recipient.viewing_pubkey,
                list_tag: None,
            }
        );
        let rotated = ViewingKey::new().pubkey();
        let enrolled = Served::new(
            &[entry(&owner)],
            Some(authorization(REGISTRY, solana_owner(&owner), rotated)),
        );
        assert_eq!(
            enrolled.resolve(&address).await.unwrap(),
            ResolvedRecipient {
                nullifier_pk: ZERO_NULLIFIER_PUBKEY,
                viewing_pubkey: rotated,
                list_tag: Some([0; 32]),
            }
        );
        assert!(matches!(
            Served::new(&[entry(&owner)], None).resolve(&address).await,
            Err(ResolveError::AuthorizationMissing)
        ));
    }

    #[test]
    fn an_opening_under_another_root_and_a_forged_absence_are_refused() {
        let owner = wallet(3);
        let member = member(&owner);
        let enrolled_tree = tree(&[entry(&owner)]);
        let address = enrolled(&owner);
        let check = |root, opening: &RingTrackingOpening| {
            RecipientResolver {
                address: &address,
                registry: REGISTRY,
                root,
            }
            .verify(&member, opening)
        };
        let honest = view(&enrolled_tree, &member);
        assert!(matches!(
            check([1; 32], &honest),
            Err(ResolveError::Opening(RecordTreeError::RootMismatch))
        ));
        let forged = view(&TrackingTree::new().unwrap(), &member);
        assert!(matches!(
            check(enrolled_tree.root().unwrap(), &forged),
            Err(ResolveError::Opening(RecordTreeError::RootMismatch))
        ));
    }

    #[tokio::test]
    async fn an_extra_opening_is_refused() {
        let owner = wallet(3);
        let mut served = Served::new(&[entry(&owner)], None);
        served.openings = 2;
        assert!(matches!(
            served.resolve(&enrolled(&owner)).await,
            Err(ResolveError::OpeningCount(2))
        ));
    }
}
