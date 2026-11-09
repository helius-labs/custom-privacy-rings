use std::sync::{Arc, Mutex};

use bytemuck::Zeroable;
use custom_ring_interface::{
    tracking::EnrollmentQueue, CustomRingProof, RingMode, REGISTER_KEY_CIPHERTEXT_LEN,
    STORAGE_PROGRAM_ID,
};
use solana_message::Hash;
use solana_transaction_status_client_types::{TransactionConfirmationStatus, TransactionStatus};
use spr_client::{AsyncProverClient, AsyncSprClient, ClientError};
use spr_keypair::{ShieldedKeypair, X25519Keypair};

use super::*;
use crate::tests::support::{
    keypair, owner_record, queued, ring, Chain, Registry, RingSettings, ServedLeaf, SLOT, TRACKING,
};
use crate::MemberRecords;

const VALID_UNTIL: u64 = 500;

fn sealed() -> EnrollIxData {
    EnrollIxData {
        nullifier_pk: [1; 32],
        list_tag: [2; 32],
        viewing_u: [3; 32],
        eph_u: [4; 32],
        ciphertext: [5; REGISTER_KEY_CIPHERTEXT_LEN],
    }
}

fn enroll_instruction(
    owner: Address,
    payer: Address,
    proof: Option<&CustomRingProof>,
) -> EnrollInstruction {
    EnrollInstruction {
        payer,
        owner,
        tracking_registry: TRACKING,
        request: EnrollRequest {
            ix: &sealed(),
            proof,
        }
        .encode(),
    }
}

fn instruction(owner: Address, payer: Address) -> EnrollInstruction {
    enroll_instruction(owner, payer, Some(&CustomRingProof::zeroed()))
}

#[tokio::test]
async fn member_records_come_back_in_member_order() {
    let registered = keypair(1);
    let stranger = keypair(2);
    let client = Chain::ring(RingSettings {
        has_policy: true,
        ..RingSettings::new(RingMode::Confidential)
    })
    .with_registry(
        Registry::new(&[(&registered, 1)]),
        &crate::tests::pipeline_rules(),
    )
    .client();
    let reads = MemberRecords {
        ring: ring(),
        members: &[owner_record(&stranger), owner_record(&registered)],
    }
    .read(client.zolana())
    .await
    .unwrap();
    assert!(reads[0].record().is_none());
    assert_eq!(reads[1].record().unwrap().lists, 1);
}

/// `Enroll` moves no lamports, the payer signs the message alone.
#[test]
fn an_enrollment_queues_on_the_pinned_registry_and_names_no_payer() {
    let owner = Address::new_from_array([3; 32]);
    let payer = Address::new_from_array([7; 32]);
    let rotation = enroll_instruction(owner, payer, None).instruction();
    let first = instruction(owner, payer).instruction();
    assert_eq!(rotation.program_id, STORAGE_PROGRAM_ID);
    assert_eq!(rotation.data[0], tag::ENROLL);
    assert_eq!(rotation.data.len(), 1 + EnrollIxData::SIZE);
    assert!(first.data.len() > rotation.data.len());
    let (queue, _) = EnrollmentQueue::find_address(&TRACKING);
    let accounts: Vec<_> = rotation
        .accounts
        .iter()
        .map(|meta| (meta.pubkey, meta.is_signer, meta.is_writable))
        .collect();
    assert_eq!(
        accounts,
        [
            (owner, true, false),
            (TRACKING, false, false),
            (queue, false, true)
        ]
    );
    assert_eq!(first.accounts, rotation.accounts);
}

const SALT: [u8; 32] = [8; 32];

fn held(owner: &ShieldedKeypair) -> ServedLeaf {
    ServedLeaf::sealed(owner.pubkey(), &owner.nullifier_key, &SALT)
}

async fn read(chain: Chain, owner: Address) -> Option<EnrollIxData> {
    EnrollmentRead {
        tracking_registry: TRACKING,
        owner,
    }
    .read(chain.client().zolana())
    .await
    .unwrap()
}

/// A queued entry applies in sequence order, a rotation ahead of the first proven entry moves nothing.
#[tokio::test]
async fn an_enrollment_reads_the_leaf_or_a_queued_proven_entry_under_the_newest_queued_key() {
    let owner = keypair(70);
    let leaf = held(&owner);
    let rotated = X25519Keypair::new().pubkey().u();
    let rotation = EnrollIxData {
        viewing_u: rotated,
        ..leaf.enroll()
    };
    let chain = || Chain::ring(RingSettings::new(RingMode::Confidential));
    assert_eq!(read(chain(), owner.pubkey()).await, None);
    let unproven = chain().with_queued(vec![queued(0, owner.pubkey(), &leaf.enroll(), false)]);
    assert_eq!(read(unproven, owner.pubkey()).await, None);
    let applied = chain().with_tracking_leaves(&[leaf]);
    assert_eq!(read(applied, owner.pubkey()).await, Some(leaf.enroll()));
    let queued_first = chain().with_queued(vec![
        queued(0, owner.pubkey(), &rotation, false),
        queued(1, owner.pubkey(), &leaf.enroll(), true),
        queued(2, keypair(71).pubkey(), &rotation, true),
    ]);
    assert_eq!(
        read(queued_first, owner.pubkey()).await,
        Some(leaf.enroll())
    );
    let rotated_after = chain()
        .with_tracking_leaves(&[leaf])
        .with_queued(vec![queued(0, owner.pubkey(), &rotation, false)]);
    assert_eq!(read(rotated_after, owner.pubkey()).await, Some(rotation));
}

fn enroll(owner: &ShieldedKeypair, viewing_u: [u8; 32]) -> Enroll<'_> {
    Enroll {
        ring: ring(),
        payer: keypair(2).pubkey(),
        owner: owner.pubkey(),
        nullifier_key: &owner.nullifier_key,
        viewing_pubkey: spr_keypair::X25519Pubkey::from_u(viewing_u).unwrap(),
        list_salt: SALT,
    }
}

/// The held key, tag and seal repeat unproven, only the viewing key moves.
#[tokio::test]
async fn an_enrolled_owner_proves_a_rotation_and_another_key_is_refused() {
    let owner = keypair(72);
    let leaf = held(&owner);
    let client = Chain::ring(RingSettings::new(RingMode::Confidential))
        .with_tracking_leaves(&[leaf])
        .client();
    let viewing_u = X25519Keypair::new().pubkey().u();
    let rotation = enroll(&owner, viewing_u).prove(&client).await.unwrap();
    let parsed = rotation.parsed().unwrap();
    assert!(parsed.proof.is_none());
    assert_eq!(
        *parsed.ix,
        EnrollIxData {
            viewing_u,
            ..leaf.enroll()
        }
    );
    let stranger = keypair(73);
    let other_key = ServedLeaf::sealed(owner.pubkey(), &stranger.nullifier_key, &SALT);
    let client = Chain::ring(RingSettings::new(RingMode::Confidential))
        .with_tracking_leaves(&[other_key])
        .client();
    assert!(matches!(
        enroll(&owner, viewing_u).prove(&client).await,
        Err(RingError::AlreadyEnrolled { owner: refused }) if refused == owner.pubkey()
    ));
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Landing {
    Landed,
    Failed,
    Absent,
}

struct LedgerState {
    sent: Vec<VersionedTransaction>,
    landing: Landing,
    expired: bool,
    preflight_refuses: bool,
}

#[derive(Clone)]
struct Ledger(Arc<Mutex<LedgerState>>);

impl Ledger {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(LedgerState {
            sent: Vec::new(),
            landing: Landing::Absent,
            expired: false,
            preflight_refuses: false,
        })))
    }

    fn set(&self, change: impl FnOnce(&mut LedgerState)) {
        change(&mut self.0.lock().unwrap());
    }

    fn sent(&self) -> Vec<VersionedTransaction> {
        self.0.lock().unwrap().sent.clone()
    }

    fn client(&self) -> RingClient<Ledger, Chain> {
        let chain = Chain::ring(RingSettings::new(RingMode::Confidential));
        RingClient::new(
            AsyncSprClient::new(self.clone(), chain, AsyncProverClient::local()),
            AsyncProverClient::local(),
        )
    }
}

#[async_trait]
impl AsyncRpc for Ledger {
    async fn get_latest_blockhash(&self) -> Result<(Hash, u64), ClientError> {
        Ok((Hash::new_unique(), VALID_UNTIL))
    }

    async fn get_block_height(&self) -> Result<u64, ClientError> {
        Ok(VALID_UNTIL + u64::from(self.0.lock().unwrap().expired))
    }

    async fn get_signature_statuses(
        &self,
        signatures: Vec<Signature>,
    ) -> Result<Vec<Option<TransactionStatus>>, ClientError> {
        let state = self.0.lock().unwrap();
        let status = |err| TransactionStatus {
            slot: SLOT,
            confirmations: None,
            status: Ok(()),
            err,
            confirmation_status: Some(TransactionConfirmationStatus::Confirmed),
        };
        Ok(signatures
            .iter()
            .map(|_| match state.landing {
                Landing::Landed => Some(status(None)),
                Landing::Failed => Some(status(Some(TransactionError::AccountNotFound))),
                Landing::Absent => None,
            })
            .collect())
    }

    async fn send_transaction_with_config(
        &self,
        transaction: &VersionedTransaction,
        _: RpcSendTransactionConfig,
    ) -> Result<Signature, ClientError> {
        let mut state = self.0.lock().unwrap();
        state.sent.push(transaction.clone());
        match state.preflight_refuses {
            true => Err(ClientError::UnsupportedRpcMethod("transport")),
            false => Ok(transaction.signatures[0]),
        }
    }
}

#[async_trait::async_trait]
impl SignatureHistory for Ledger {
    async fn signature_statuses_with_history(
        &self,
        signatures: Vec<Signature>,
    ) -> Result<Vec<Option<TransactionStatus>>, ClientError> {
        self.get_signature_statuses(signatures).await
    }
}

/// Each record keeps the ledger's sent count at that moment.
struct Journal {
    ledger: Ledger,
    recorded: Mutex<Vec<(Vec<u8>, usize)>>,
    fails: bool,
}

impl Journal {
    fn new(ledger: &Ledger) -> Self {
        Self {
            ledger: ledger.clone(),
            recorded: Mutex::new(Vec::new()),
            fails: false,
        }
    }

    fn last(&self) -> EnrollmentAttempt {
        let recorded = self.recorded.lock().unwrap();
        EnrollmentAttempt::decode(&recorded.last().unwrap().0).unwrap()
    }
}

#[async_trait]
impl EnrollmentJournal for Journal {
    async fn record(
        &self,
        attempt: &EnrollmentAttempt,
    ) -> Result<(), Box<dyn StdError + Send + Sync>> {
        if self.fails {
            return Err("disk full".into());
        }
        let encoded = attempt.encode()?;
        self.recorded
            .lock()
            .unwrap()
            .push((encoded, self.ledger.sent().len()));
        Ok(())
    }
}

fn wallets() -> (ShieldedKeypair, ShieldedKeypair) {
    (keypair(1), keypair(2))
}

#[tokio::test]
async fn an_attempt_is_recorded_before_its_broadcast_and_signed_by_owner_and_payer() {
    let ledger = Ledger::new();
    let journal = Journal::new(&ledger);
    let (owner, payer) = wallets();

    let attempt = instruction(owner.pubkey(), payer.pubkey())
        .submit(&ledger.client(), &[&payer, &owner], &journal)
        .await
        .unwrap();

    let recorded = journal.recorded.lock().unwrap().clone();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0].1, 0, "nothing left before the record");
    assert_eq!(ledger.sent(), std::slice::from_ref(&attempt.transaction));
    assert_eq!(attempt.transaction.signatures.len(), 2);
    assert_eq!(
        attempt.transaction.message.static_account_keys()[0],
        payer.pubkey()
    );
}

#[tokio::test]
async fn an_unrecorded_attempt_sends_nothing() {
    let ledger = Ledger::new();
    let journal = Journal {
        fails: true,
        ..Journal::new(&ledger)
    };
    let (owner, payer) = wallets();

    let refused = instruction(owner.pubkey(), payer.pubkey())
        .submit(&ledger.client(), &[&payer, &owner], &journal)
        .await;

    assert!(matches!(refused, Err(RingError::AttemptNotRecorded(_))));
    assert!(ledger.sent().is_empty());
}

/// A transport error after the record leaves the outcome to reconciliation, never to a new send.
#[tokio::test]
async fn an_in_doubt_attempt_reconciles_from_its_record_after_a_restart() {
    let ledger = Ledger::new();
    ledger.set(|state| state.preflight_refuses = true);
    let journal = Journal::new(&ledger);
    let (owner, payer) = wallets();
    let client = ledger.client();
    let sent = instruction(owner.pubkey(), payer.pubkey())
        .submit(&client, &[&payer, &owner], &journal)
        .await;
    assert!(sent.is_ok(), "a lost answer is in doubt, not refused");
    let restored = journal.last();

    let outcome = || async { restored.reconcile(&client).await.unwrap() };
    assert_eq!(outcome().await, EnrollmentOutcome::Pending);
    assert!(matches!(
        restored
            .clone()
            .resubmit(&client, &[&payer, &owner], &journal)
            .await,
        Err(RingError::AttemptNotExpired(EnrollmentOutcome::Pending))
    ));
    ledger.set(|state| state.landing = Landing::Failed);
    assert_eq!(
        outcome().await,
        EnrollmentOutcome::Failed(TransactionError::AccountNotFound)
    );
    ledger.set(|state| state.landing = Landing::Landed);
    assert_eq!(outcome().await, EnrollmentOutcome::Landed { slot: SLOT });
    assert_eq!(ledger.sent().len(), 1);
}

#[tokio::test]
async fn only_an_expired_attempt_goes_again_with_the_same_request() {
    let ledger = Ledger::new();
    let journal = Journal::new(&ledger);
    let (owner, payer) = wallets();
    let client = ledger.client();
    let first = instruction(owner.pubkey(), payer.pubkey())
        .submit(&client, &[&payer, &owner], &journal)
        .await
        .unwrap();
    assert!(matches!(
        first
            .clone()
            .resubmit(&client, &[&payer, &owner], &journal)
            .await,
        Err(RingError::AttemptNotExpired(EnrollmentOutcome::Pending))
    ));
    ledger.set(|state| state.expired = true);
    assert_eq!(
        first.reconcile(&client).await.unwrap(),
        EnrollmentOutcome::Expired
    );

    let second = first
        .clone()
        .resubmit(&client, &[&payer, &owner], &journal)
        .await
        .unwrap();

    assert_eq!(second.instruction, first.instruction);
    assert_ne!(second.signature(), first.signature());
    assert_eq!(journal.recorded.lock().unwrap().len(), 2);
}
