use custom_ring_interface::RingMode;
use solana_signer::Signer;
use spr_interface::pda;
use spr_keypair::ShieldedKeypair;
use spr_transaction::Mint;

use super::support::{keypair, ring, ring_utxo, zero_key, Chain, RingSettings, TREE_ID};
use crate::{
    CustomRingMerge, FittedMerge, MergeProofInput, PreparedCustomRingMerge, RingError,
    RingMergeOperation, RingSubmission,
};

struct Merge<'a> {
    owner: &'a ShieldedKeypair,
    settings: RingSettings,
    payer: &'a ShieldedKeypair,
    signers: &'a [&'a (dyn Signer + Sync)],
}

fn prepared(owner: &ShieldedKeypair) -> PreparedCustomRingMerge {
    let utxos = (0..2)
        .map(|leaf| ring_utxo(owner, Mint::SOL, 5, leaf))
        .collect();
    CustomRingMerge::new(ring(), utxos, None)
        .unwrap()
        .encrypt(owner)
        .unwrap()
}

impl Merge<'_> {
    async fn refusal(self) -> RingError {
        let merge = RingMergeOperation::new(
            prepared(self.owner),
            MergeProofInput {
                nullifier_key: self.owner.nullifier_key.clone(),
                input_tree: pda::tree(TREE_ID),
                output_tree: pda::tree(TREE_ID),
            },
        );
        RingSubmission::new(merge)
            .prove(
                &Chain::ring(self.settings).client(),
                self.payer,
                self.signers,
            )
            .await
            .map(<[_]>::len)
            .unwrap_err()
    }
}

/// The hidden owner never pays, signs or co-signs its anonymous merge.
#[tokio::test]
async fn an_anonymous_merge_its_owner_would_sign_in_public_is_refused_before_proving() {
    let owner = keypair(1);
    let unsigned: &[&(dyn Signer + Sync)] = &[];
    let anonymous = RingSettings::new(RingMode::Anonymous);
    let refusal = |settings, payer, signers| {
        Merge {
            owner: &owner,
            settings,
            payer,
            signers,
        }
        .refusal()
    };
    assert!(matches!(
        refusal(anonymous, &owner, unsigned).await,
        RingError::AnonymousPublicSignerLinksOwner
    ));
    let payer = keypair(9);
    assert!(matches!(
        refusal(anonymous, &payer, &[&owner]).await,
        RingError::AnonymousPublicSignerLinksOwner
    ));
    let confidential = ring()
        .require_config(&Chain::ring(RingSettings::new(RingMode::Confidential)))
        .await
        .unwrap();
    assert!(prepared(&owner)
        .check_public_signers([owner.pubkey()], &confidential)
        .is_ok());
}

/// The merge circuit admits its owner as payer, only the wallet refuses one.
#[tokio::test]
async fn a_confidential_merge_its_owner_pays_is_refused_before_proving() {
    let owner = keypair(1);
    let merge = Merge {
        owner: &owner,
        settings: RingSettings::new(RingMode::Confidential),
        payer: &owner,
        signers: &[],
    };
    let refusal = merge.refusal().await;
    assert!(
        matches!(refusal, RingError::AnonymousPublicSignerLinksOwner),
        "{refusal}"
    );
}

/// Compact padding publishes nothing, the widest circuit carries what the bytes and accounts allow.
#[tokio::test]
async fn a_merge_takes_every_input_one_transaction_carries() {
    use solana_address::Address;
    let owner = keypair(1);
    let chain = Chain::ring(RingSettings::new(RingMode::Confidential));
    let input = MergeProofInput {
        nullifier_key: owner.nullifier_key.clone(),
        input_tree: pda::tree(TREE_ID),
        output_tree: pda::tree(TREE_ID),
    };
    let payer = Address::new_from_array([7; 32]);
    let notes = |count: u64| -> Vec<_> {
        (0..count)
            .map(|leaf| ring_utxo(&owner, Mint::SOL, 5, leaf))
            .collect()
    };
    let holder = CountingHolder::of(&owner, owner.signing_pubkey());
    let carried = FittedMerge {
        ring: ring(),
        inputs: notes(crate::MAX_RING_MERGE_INPUTS as u64),
        output_tree_id: TREE_ID,
        keys: &holder,
        proof_input: &input,
        payer,
    }
    .prepare(&chain)
    .await
    .unwrap();
    let published = carried.published_inputs();
    assert_eq!(published, WIDEST_MERGE);
    assert_eq!(holder.calls.get(), 1);

    let config = ring().require_config(&chain).await.unwrap();
    let one_more = CustomRingMerge::new(ring(), notes(published as u64 + 1), None)
        .unwrap()
        .with_output_tree_id(TREE_ID)
        .encrypt(&owner)
        .unwrap();
    assert!(matches!(
        crate::submission::check_transaction_size(
            &payer,
            &one_more
                .preview_instruction(&input, payer, &config)
                .unwrap(),
            crate::submission::send_budget(custom_ring_interface::InstructionTag::Merge, None),
        ),
        Err(RingError::TransactionTooLarge { .. })
    ));
}

/// Inputs a keyed merge carries in one v1 transaction, measured here.
const WIDEST_MERGE: usize = 44;

/// Every derivation under the zero secret is public.
#[test]
fn a_zero_key_merge_is_refused_before_any_derivation() {
    use spr_interface::ZERO_NULLIFIER_PUBKEY;
    let owner = keypair(1);
    let utxos: Vec<_> = (0..2)
        .map(|leaf| zero_key(ring_utxo(&owner, Mint::SOL, 5, leaf)))
        .collect();
    let viewing = spr_keypair::ViewingKey::new();
    let keys = spr_transaction::LocalShieldedKeys::new(
        spr_keypair::ShieldedAddress {
            signing_pubkey: owner.signing_pubkey(),
            nullifier_pubkey: ZERO_NULLIFIER_PUBKEY,
            viewing_pubkey: viewing.pubkey(),
        },
        vec![viewing],
        spr_keypair::NullifierKey::from_secret([0; 31]),
    )
    .unwrap();
    assert!(matches!(
        CustomRingMerge::new(ring(), utxos, None)
            .unwrap()
            .encrypt(&keys),
        Err(RingError::MergeOwnerKey)
    ));
}

#[test]
fn a_zero_key_merge_relay_is_refused_before_encoding() {
    use crate::rpc::{RelayQuote, ServiceTarget};
    use spr_interface::ZERO_NULLIFIER_PUBKEY;
    let owner = keypair(1);
    let utxos: Vec<_> = (0..2)
        .map(|leaf| zero_key(ring_utxo(&owner, Mint::SOL, 5, leaf)))
        .collect();
    let viewing = spr_keypair::ViewingKey::new();
    let sender = spr_keypair::ShieldedAddress {
        signing_pubkey: owner.signing_pubkey(),
        nullifier_pubkey: ZERO_NULLIFIER_PUBKEY,
        viewing_pubkey: viewing.pubkey(),
    };
    let prepared = crate::MergeRelay {
        ring: ring(),
        service: ServiceTarget {
            cluster: [0x11; 32],
            service: solana_address::Address::new_from_array([0x22; 32]),
        },
        sender,
        owner: &owner,
        nullifier_key: spr_keypair::NullifierKey::from_secret([0; 31]),
        inputs: utxos,
    }
    .prepare(RelayQuote {
        fee_payer: solana_address::Address::new_from_array([0x33; 32]).into(),
        valid_until_slot: 100,
    });
    assert!(matches!(prepared, Err(RingError::MergeOwnerKey)));
}

struct CountingHolder<'a> {
    keys: &'a ShieldedKeypair,
    address: spr_keypair::ShieldedAddress,
    calls: core::cell::Cell<usize>,
}

impl<'a> CountingHolder<'a> {
    fn of(keys: &'a ShieldedKeypair, signing_pubkey: spr_keypair::PublicKey) -> Self {
        Self {
            keys,
            address: spr_keypair::ShieldedAddress {
                signing_pubkey,
                ..keys.shielded_address().unwrap()
            },
            calls: Default::default(),
        }
    }

    /// A note of the address, its commitment and nullifier follow its owner.
    fn note(&self, leaf: u64) -> spr_transaction::WalletUtxo {
        let mut note = ring_utxo(self.keys, Mint::SOL, 5, leaf);
        note.utxo.owner = self.address.signing_pubkey;
        note.utxo_hash = note
            .utxo
            .hash(&note.nullifier_pubkey, &[0; 32], &[0; 32], note.tree_id)
            .unwrap();
        note.nullifier = self
            .keys
            .nullifier_key
            .nullifier(&note.utxo_hash, &note.utxo.blinding)
            .unwrap();
        note
    }

    fn count(&self) {
        self.calls.set(self.calls.get() + 1);
    }
}

impl spr_transaction::ShieldedKeys for CountingHolder<'_> {
    fn address(&self) -> Result<spr_keypair::ShieldedAddress, spr_transaction::TransactionError> {
        Ok(self.address)
    }

    fn viewing_public_keys(&self) -> Vec<spr_keypair::X25519Pubkey> {
        self.keys.viewing_public_keys()
    }

    fn decrypt(
        &self,
        requests: &[spr_transaction::DecryptRequest<'_>],
    ) -> Result<Vec<Vec<u8>>, spr_transaction::TransactionError> {
        self.count();
        self.keys.decrypt(requests)
    }

    fn derive(
        &self,
        requests: &[spr_transaction::DeriveRequest],
    ) -> Result<Vec<[u8; 32]>, spr_transaction::TransactionError> {
        self.count();
        self.keys.derive(requests)
    }

    fn transaction_keys(
        &self,
        requests: &[spr_transaction::TransactionKeyRequest],
    ) -> Result<Vec<spr_keypair::ViewingKey>, spr_transaction::TransactionError> {
        self.count();
        self.keys.transaction_keys(requests)
    }
}

fn proof_input(owner: &ShieldedKeypair) -> MergeProofInput {
    MergeProofInput {
        nullifier_key: owner.nullifier_key.clone(),
        input_tree: pda::tree(TREE_ID),
        output_tree: pda::tree(TREE_ID),
    }
}

/// A note past the fitted prefix refuses as one inside it would, before the holder derives anything.
#[tokio::test]
async fn a_merge_refuses_an_input_past_its_fitted_prefix() {
    use solana_address::Address;
    use spr_transaction::TransactionError;
    let owner = keypair(1);
    let chain = Chain::ring(RingSettings::new(RingMode::Confidential));
    let input = proof_input(&owner);
    let holder = CountingHolder::of(&owner, owner.signing_pubkey());
    let fitted = |inputs| FittedMerge {
        ring: ring(),
        inputs,
        output_tree_id: TREE_ID,
        keys: &holder,
        proof_input: &input,
        payer: Address::new_from_array([7; 32]),
    };
    let widest = crate::MAX_RING_MERGE_INPUTS;
    let mut foreign_tail: Vec<_> = (0..widest as u64).map(|leaf| holder.note(leaf)).collect();
    let foreign_mint = Mint::new(Address::new_from_array([0x3c; 32]), 9);
    foreign_tail[widest - 1] = ring_utxo(&owner, foreign_mint, 5, widest as u64);
    assert!(matches!(
        fitted(Vec::new()).prepare(&chain).await,
        Err(RingError::Transaction(TransactionError::NoInputs))
    ));
    assert!(matches!(
        fitted((0..=widest as u64).map(|leaf| holder.note(leaf)).collect())
            .prepare(&chain)
            .await,
        Err(RingError::Transaction(TransactionError::TooManyInputs { got, .. })) if got == widest + 1
    ));
    assert!(matches!(
        fitted(foreign_tail).prepare(&chain).await,
        Err(RingError::Transaction(TransactionError::MergeInputAssetMismatch { index }))
            if index == widest - 1
    ));
    assert_eq!(holder.calls.get(), 0);
}

/// A PDA owner stays hidden behind its keyed merge, no EdDSA proof names it.
#[tokio::test]
async fn a_pda_owns_a_keyed_merge_on_both_modes() {
    use solana_address::Address;
    let program_owner = Address::new_from_array([0x51; 32]);
    let keys = keypair(1);
    let input = proof_input(&keys);
    let payer = Address::new_from_array([7; 32]);
    for mode in [RingMode::Confidential, RingMode::Anonymous] {
        let chain = Chain::ring(RingSettings::new(mode));
        let config = ring().require_config(&chain).await.unwrap();
        let holder = CountingHolder::of(&keys, spr_keypair::PublicKey::from_pda(&program_owner));
        let keyed = FittedMerge {
            ring: ring(),
            inputs: (0..2).map(|leaf| holder.note(leaf)).collect(),
            output_tree_id: TREE_ID,
            keys: &holder,
            proof_input: &input,
            payer,
        }
        .prepare(&chain)
        .await
        .unwrap();
        assert!(keyed.check_public_signers([payer], &config).is_ok());
        let linked = keyed.check_public_signers([payer, program_owner], &config);
        match mode {
            RingMode::Anonymous => assert!(matches!(
                linked,
                Err(RingError::AnonymousPublicSignerLinksOwner)
            )),
            RingMode::Confidential => assert!(linked.is_ok()),
        }
    }
}
