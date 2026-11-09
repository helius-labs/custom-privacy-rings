use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use custom_ring_interface::{
    storage_instruction::{tag, EnrollRequest},
    RingMode, STORAGE_PROGRAM_ID,
};
use solana_message::Hash;
use solana_transaction::versioned::VersionedTransaction;
use spr_client::{AsyncProverClient, AsyncSprClient, ClientError};
use spr_interface::ZERO_NULLIFIER_PUBKEY;
use spr_keypair::X25519Keypair;
use spr_keypair::{ShieldedAddress, ShieldedKeypair, ViewingKey};
use zolana_ring_client::ViewingRotation;
use zolana_ring_keys::RingAddress;

use super::*;
use crate::tests::support::{keypair, ring, Chain, RingSettings, ServedLeaf};
use crate::SaltedOwnerCheck;

/// The chain with each landed rotation applied to the owner's leaf.
struct Ledger {
    chain: Arc<Mutex<Chain>>,
    leaf: ServedLeaf,
    sent: Arc<Mutex<usize>>,
}

#[async_trait]
impl AsyncRpc for Ledger {
    async fn get_account(
        &self,
        address: solana_address::Address,
    ) -> Result<Option<solana_account::Account>, ClientError> {
        let chain = self.chain.lock().unwrap().clone();
        chain.get_account(address).await
    }

    async fn get_latest_blockhash(&self) -> Result<(Hash, u64), ClientError> {
        Ok((Hash::default(), 0))
    }

    async fn process_transaction(
        &self,
        transaction: VersionedTransaction,
    ) -> Result<Signature, ClientError> {
        let keys = transaction.message.static_account_keys();
        let [instruction] = transaction.message.instructions() else {
            panic!("one instruction");
        };
        assert_eq!(
            keys[usize::from(instruction.program_id_index)],
            STORAGE_PROGRAM_ID
        );
        let (&enroll, request) = instruction.data.split_first().unwrap();
        assert_eq!(enroll, tag::ENROLL);
        let request = EnrollRequest::parse(request).unwrap();
        assert!(request.proof.is_none());
        assert_eq!(keys[usize::from(instruction.accounts[0])], self.leaf.owner);
        let applied = ServedLeaf {
            viewing_u: request.ix.viewing_u,
            ..self.leaf
        };
        let mut chain = self.chain.lock().unwrap();
        *chain = chain.clone().with_tracking_leaves(&[applied]);
        *self.sent.lock().unwrap() += 1;
        Ok(transaction.signatures[0])
    }
}

fn rotation<'a>(
    identity: &'a ShieldedKeypair,
    keys: &'a RotatingRingKeys,
    history: &'a ViewingHistory,
) -> RotateViewingKey<'a> {
    RotateViewingKey {
        ring: ring(),
        owner: identity,
        payer: identity,
        keys,
        history,
    }
}

#[tokio::test]
async fn a_rotated_key_resolves_for_a_sender_and_a_repeat_sends_nothing() {
    let identity = keypair(3);
    let address = identity.shielded_address().unwrap();
    let leaf = ServedLeaf::of(&address, [0x06; 32]);
    let chain = Arc::new(Mutex::new(
        Chain::ring(RingSettings::new(RingMode::Confidential)).with_tracking_leaves(&[leaf]),
    ));
    let sent = Arc::new(Mutex::new(0));
    let client = ledger_client(Ledger {
        chain: Arc::clone(&chain),
        leaf,
        sent: Arc::clone(&sent),
    });
    let start = ViewingRotation {
        keys: RotatingRingKeys::new(&identity).unwrap(),
        history: ViewingHistory::new(identity.viewing_pubkey(), 0),
    };
    let ahead = start.keys.clone().rotate(ViewingKey::new()).unwrap();
    assert!(matches!(
        rotation(&identity, &ahead, &start.history)
            .send(&client)
            .await,
        Err(RingError::ViewingHistoryNotCurrent)
    ));

    let rotated = start.rotate(ViewingKey::new(), 1).unwrap();
    let new = rotated.keys.address().unwrap().viewing_pubkey;
    let landed = rotation(&identity, &rotated.keys, &rotated.history)
        .send(&client)
        .await
        .unwrap();
    assert!(landed.signature.is_some());
    assert_eq!(landed.viewing_pubkey, new);
    let held = RingAddress {
        recipient: ShieldedAddress {
            nullifier_pubkey: ZERO_NULLIFIER_PUBKEY,
            ..address
        },
        ring_program_id: ring().program_id(),
        list_salt: [0; 32],
    };
    let indexer = chain.lock().unwrap().clone();
    let resolved = SaltedOwnerCheck {
        ring: ring(),
        address: &held,
    }
    .run(&indexer)
    .await
    .unwrap();
    assert_eq!(resolved.viewing_pubkey, new);

    let repeated = rotation(&identity, &rotated.keys, &rotated.history)
        .send(&ledger_client(Ledger {
            chain: Arc::clone(&chain),
            leaf,
            sent: Arc::clone(&sent),
        }))
        .await
        .unwrap();
    assert_eq!(repeated.signature, None);
    assert_eq!(*sent.lock().unwrap(), 1);
}

/// The indexer reads the chain the ledger last applied.
fn ledger_client(ledger: Ledger) -> RingClient<Ledger, Chain> {
    let chain = ledger.chain.lock().unwrap().clone();
    RingClient::new(
        AsyncSprClient::new(ledger, chain, AsyncProverClient::local()),
        AsyncProverClient::local(),
    )
}

#[tokio::test]
async fn a_rotation_refuses_an_unenrolled_owner_and_a_leaf_key_the_wallet_never_held() {
    let identity = keypair(3);
    let address = identity.shielded_address().unwrap();
    let start = ViewingRotation {
        keys: RotatingRingKeys::new(&identity).unwrap(),
        history: ViewingHistory::new(identity.viewing_pubkey(), 0),
    };
    let send = |leaves: Vec<ServedLeaf>| {
        let chain =
            Chain::ring(RingSettings::new(RingMode::Confidential)).with_tracking_leaves(&leaves);
        let client = ledger_client(Ledger {
            chain: Arc::new(Mutex::new(chain)),
            leaf: ServedLeaf::of(&address, [0x06; 32]),
            sent: Arc::default(),
        });
        let start = &start;
        let identity = &identity;
        async move {
            rotation(identity, &start.keys, &start.history)
                .send(&client)
                .await
        }
    };
    assert!(matches!(
        send(Vec::new()).await,
        Err(RingError::RotationUnenrolled { .. })
    ));
    let foreign = ServedLeaf {
        viewing_u: X25519Keypair::new().pubkey().u(),
        ..ServedLeaf::of(&address, [0x06; 32])
    };
    assert!(matches!(
        send(vec![foreign]).await,
        Err(RingError::ViewingKeyNotHeld { .. })
    ));
}
