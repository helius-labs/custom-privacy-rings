//! The key a delegate move spends a source's notes under, and the notes the move may spend. The
//! public zero key needs no enrollment, any other key comes only from the source's enrollment and
//! must derive the address the request names.
//!
//! Recovered notes pass [`ResolvedSource::check_notes`] before they reach a
//! [`DelegateSource`](crate::DelegateSource), or [`AllowedNullifiers::admit`] per requested source
//! on the relay path.

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use solana_address::Address;
use spr_client::{AsyncRpc, ClientError};
use spr_interface::ZERO_NULLIFIER_PUBKEY;
use spr_keypair::{
    KeypairError, NullifierKey, ShieldedAddress, X25519Keypair, SHIELDED_ADDRESS_LEN,
};
use spr_transaction::{TransactionError, WalletUtxo};
use thiserror::Error;

use crate::{
    pipeline::same_owner, AsyncRingIndexer, AuditError, MessageMismatch, RegisteredKeys,
    ResolveError,
};

#[derive(Debug, Error)]
pub enum DelegateSourceError {
    #[error("the source wallet has no enrolled nullifier key")]
    Unenrolled,
    #[error("the enrolled key derives another nullifier key than the source address names")]
    KeyMismatch,
    #[error("the enrollment seal opens to another key than its record holds")]
    SealMismatch,
    #[error("note {index} belongs to another owner or key than the source")]
    ForeignNote { index: usize },
    #[error("note {index} does not rebuild the hash or nullifier it carries")]
    UnboundOpening { index: usize },
    #[error("the allowed notes cover other sources than the request names")]
    SourcesMismatch,
    #[error(
        "the move spends {spent} notes, {allowed} of them derive from a requested source opening"
    )]
    UnrequestedInput { spent: u8, allowed: usize },
    #[error(transparent)]
    Keypair(#[from] KeypairError),
    #[error(transparent)]
    Transaction(#[from] TransactionError),
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error(transparent)]
    Seal(#[from] AuditError),
    #[error(transparent)]
    Tracking(#[from] ResolveError),
}

/// The single enrollment read every nonzero source resolves through.
#[async_trait]
pub trait SourceEnrollment: Sync {
    type Error: From<DelegateSourceError> + Send;

    /// `None` when `source` holds no enrollment.
    async fn escrowed_key(
        &self,
        source: &ShieldedAddress,
    ) -> Result<Option<NullifierKey>, Self::Error>;
}

/// The auditor opens the seal of the source's tracking leaf under the registry's newest root, only an enrolled owner holds one.
pub struct TrackingEnrollment<'a, R> {
    pub rpc: &'a R,
    pub tracking_registry: Address,
    pub auditor: &'a X25519Keypair,
}

#[async_trait]
impl<R: AsyncRpc + AsyncRingIndexer> SourceEnrollment for TrackingEnrollment<'_, R> {
    type Error = DelegateSourceError;

    async fn escrowed_key(
        &self,
        source: &ShieldedAddress,
    ) -> Result<Option<NullifierKey>, DelegateSourceError> {
        let owner = source.solana_address()?.to_bytes();
        let mut leaves = RegisteredKeys {
            registry: self.tracking_registry,
            owners: &[owner],
        }
        .leaves(self.rpc, self.rpc)
        .await?;
        leaves
            .remove(&owner)
            .map(|leaf| leaf.nullifier_key(self.auditor))
            .transpose()
            .map_err(Into::into)
    }
}

/// Only [`ResolvedSource::resolve`] builds one, the key is the zero key or the enrolled key.
pub struct ResolvedSource {
    address: ShieldedAddress,
    nullifier_key: NullifierKey,
}

impl ResolvedSource {
    pub async fn resolve<E: SourceEnrollment + ?Sized>(
        address: ShieldedAddress,
        enrollment: &E,
    ) -> Result<Self, E::Error> {
        // 1. The zero key's secret is public, its notes need no escrow.
        let nullifier_key = if address.nullifier_pubkey == ZERO_NULLIFIER_PUBKEY {
            NullifierKey::from_secret([0; 31])
        } else {
            enrollment
                .escrowed_key(&address)
                .await?
                .ok_or(DelegateSourceError::Unenrolled)?
        };
        // 2. An enrollment under another key would spend notes the named address does not hold.
        let derived = nullifier_key.pubkey().map_err(DelegateSourceError::from)?;
        if derived != address.nullifier_pubkey {
            return Err(DelegateSourceError::KeyMismatch.into());
        }
        Ok(Self {
            address,
            nullifier_key,
        })
    }

    /// Refuses the first note another owner holds or another key nullifies.
    pub fn check_notes(&self, notes: &[WalletUtxo]) -> Result<(), DelegateSourceError> {
        self.nullifiers(notes).map(drop)
    }

    fn nullifiers(&self, notes: &[WalletUtxo]) -> Result<Vec<[u8; 32]>, DelegateSourceError> {
        notes
            .iter()
            .enumerate()
            .map(|(index, note)| self.nullifier_of(index, note))
            .collect()
    }

    fn nullifier_of(
        &self,
        index: usize,
        note: &WalletUtxo,
    ) -> Result<[u8; 32], DelegateSourceError> {
        // 1. Owner and key are opening fields the rebuilt hash commits, so a match binds the note.
        if !same_owner(&note.utxo.owner, &self.address.signing_pubkey)
            || note.nullifier_pubkey != self.address.nullifier_pubkey
        {
            return Err(DelegateSourceError::ForeignNote { index });
        }
        // 2. Rebuilt, not read, any carried hash yields its live nullifier under the shared zero key.
        let rebuilt = note.utxo.hash(
            &note.nullifier_pubkey,
            &note.data_hash.unwrap_or_default(),
            &note.ring_data_hash.unwrap_or_default(),
            note.tree_id,
        )?;
        let nullifier = self
            .nullifier_key
            .nullifier(&rebuilt, &note.utxo.blinding)?;
        // 3. Selection and the prover read the carried values, they must be the rebuilt ones.
        if rebuilt != note.utxo_hash || nullifier != note.nullifier {
            return Err(DelegateSourceError::UnboundOpening { index });
        }
        Ok(nullifier)
    }

    pub fn nullifier_key(&self) -> &NullifierKey {
        &self.nullifier_key
    }

    pub fn into_nullifier_key(self) -> NullifierKey {
        self.nullifier_key
    }
}

/// Filled only by [`Self::admit`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AllowedNullifiers {
    sources: BTreeSet<[u8; SHIELDED_ADDRESS_LEN]>,
    notes: BTreeMap<[u8; 32], AllowedNote>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AllowedNote {
    held: SourceMint,
    amount: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SourceMint {
    pub owner_key: [u8; 32],
    pub nullifier_pk: [u8; 32],
    pub mint: [u8; 32],
}

/// What a frame spends of each requested source per mint, less the change it returns there.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceSpends(BTreeMap<SourceMint, i128>);

pub(crate) struct MintInputCheck<'a> {
    pub expected: &'a crate::relay_message::ExpectedDelegate,
    pub mint: Address,
    pub frame: &'a custom_ring_interface::RingTransactData<'a>,
    pub real_inputs: u8,
    pub records: &'a [(zolana_ring_keys::OutputRecord, spr_keypair::X25519Pubkey)],
}

impl MintInputCheck<'_> {
    pub fn spends(self) -> Result<SourceSpends, MessageMismatch> {
        use custom_ring_interface::RecordKind;
        use spr_hasher::primitives::is_canonical_bn254_scalar_be;
        use spr_keypair::PublicKey;
        use spr_transaction::{
            instructions::transact::PrivateTxHash, utxo::derive_private_tx_blinding, Mint, Utxo,
        };

        let refused = MessageMismatch::SpendsUnrequestedNote;
        let openings = &self.expected.input_openings;
        if self.real_inputs == 0
            || usize::from(self.real_inputs) != openings.len()
            || self.frame.outputs.len() != self.records.len()
        {
            return Err(refused);
        }
        let mut spends = SourceSpends::default();
        let hashes = openings
            .iter()
            .map(|opening| {
                let source = self
                    .expected
                    .sources
                    .get(usize::from(opening.source_index))
                    .ok_or(refused)?;
                let ring_data_hash = opening
                    .ring_data_hash
                    .as_ref()
                    .map_or([0; 32], |hash| hash.0);
                if !is_canonical_bn254_scalar_be(&source.nullifier_pk)
                    || !is_canonical_bn254_scalar_be(&opening.blinding.0)
                    || !is_canonical_bn254_scalar_be(&ring_data_hash)
                {
                    return Err(refused);
                }
                let hash = Utxo {
                    owner: PublicKey::from_ed25519(&source.owner_key),
                    asset: Mint::new(self.mint, 0),
                    amount: opening.amount,
                    blinding: opening.blinding.0,
                    ring_program_id: Some(self.expected.ring.program_id()),
                    data: Default::default(),
                }
                .hash(
                    &source.nullifier_pk,
                    &[0; 32],
                    &ring_data_hash,
                    opening.tree_id,
                )
                .map_err(|_| refused)?;
                spends.credit(
                    SourceMint {
                        owner_key: source.owner_key,
                        nullifier_pk: source.nullifier_pk,
                        mint: self.mint.to_bytes(),
                    },
                    opening.amount,
                );
                Ok(hash)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let outputs: Vec<_> = self
            .frame
            .outputs
            .iter()
            .zip(self.records)
            .filter(|(_, (record, _))| record.kind != RecordKind::Dummy)
            .map(|(output, _)| *output.hash)
            .collect();
        let key = self.expected.tx_viewing_key.key().map_err(|_| refused)?;
        let seed = zeroize::Zeroizing::new(
            zolana_ring_keys::ring_blinding_seed(&key.secret_bytes(), self.frame.salt)
                .map_err(|_| refused)?,
        );
        let blinding = zeroize::Zeroizing::new(
            derive_private_tx_blinding(self.frame.first_nullifier().map_err(|_| refused)?, &seed)
                .map_err(|_| refused)?,
        );
        let hash = PrivateTxHash::new(&hashes, &outputs, &blinding)
            .hash()
            .map_err(|_| refused)?;
        if hash != *self.frame.private_tx_hash {
            return Err(refused);
        }
        Ok(spends)
    }
}

impl AllowedNullifiers {
    /// Admits nothing of `source` when one of its openings fails.
    pub fn admit(
        &mut self,
        source: &ResolvedSource,
        notes: &[WalletUtxo],
    ) -> Result<(), DelegateSourceError> {
        let admitted = source
            .nullifiers(notes)?
            .into_iter()
            .zip(notes)
            .map(|(nullifier, note)| Ok((nullifier, AllowedNote::of(note)?)))
            .collect::<Result<Vec<_>, DelegateSourceError>>()?;
        self.sources.insert(source.address.to_bytes());
        self.notes.extend(admitted);
        Ok(())
    }

    pub fn check_sources<'s>(
        &self,
        requested: impl IntoIterator<Item = &'s ShieldedAddress>,
    ) -> Result<(), DelegateSourceError> {
        let requested: BTreeSet<_> = requested
            .into_iter()
            .map(ShieldedAddress::to_bytes)
            .collect();
        (requested == self.sources)
            .then_some(())
            .ok_or(DelegateSourceError::SourcesMismatch)
    }

    /// The proof names `spent` notes and no position, the distinct admitted nullifiers must count
    /// them all.
    pub fn check_spends<'n>(
        &self,
        spent: u8,
        published: impl IntoIterator<Item = &'n [u8; 32]>,
    ) -> Result<SourceSpends, DelegateSourceError> {
        let admitted: Vec<&AllowedNote> = published
            .into_iter()
            .filter_map(|nullifier| self.notes.get(nullifier))
            .collect();
        if admitted.len() != usize::from(spent) {
            return Err(DelegateSourceError::UnrequestedInput {
                spent,
                allowed: admitted.len(),
            });
        }
        let mut spends = SourceSpends::default();
        for note in admitted {
            spends.credit(note.held, note.amount);
        }
        Ok(spends)
    }
}

impl AllowedNote {
    /// Read only after the opening rebuilds the hash and nullifier these fields commit.
    fn of(note: &WalletUtxo) -> Result<Self, DelegateSourceError> {
        Ok(Self {
            held: SourceMint {
                owner_key: note.utxo.owner.confidential_view_tag()?,
                nullifier_pk: note.nullifier_pubkey,
                mint: note.utxo.asset.asset.to_bytes(),
            },
            amount: note.utxo.amount,
        })
    }
}

impl SourceSpends {
    fn credit(&mut self, held: SourceMint, amount: u64) {
        *self.0.entry(held).or_default() += i128::from(amount);
    }

    pub(crate) fn debit(&mut self, held: SourceMint, amount: u64) {
        *self.0.entry(held).or_default() -= i128::from(amount);
    }

    /// A source's change in a mint is at most what the frame spends of it there.
    pub(crate) fn check_change(&self) -> Result<(), MessageMismatch> {
        self.0
            .values()
            .all(|left| *left >= 0)
            .then_some(())
            .ok_or(MessageMismatch::ChangeExceedsSpend)
    }
}

#[cfg(test)]
#[path = "../tests/transfer/delegate_source.rs"]
pub(crate) mod tests;
