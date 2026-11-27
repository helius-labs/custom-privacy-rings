//! A ring's durable projection, what its auditor opened and the frames it could not read.
//!
//! The lease holder writes it, each commit fenced by the lease. Every replica reads it from one
//! snapshot, so a load never pairs a newer checkpoint with an older spend.

use custom_ring_interface::REGISTER_KEY_CIPHERTEXT_LEN;
use sea_orm::{
    AccessMode, ConnectionTrait, DatabaseBackend, DatabaseConnection, DatabaseTransaction, DbErr,
    IsolationLevel, QueryResult, TransactionTrait, Value,
};
use sha2::Sha256;
use solana_address::Address;
use solana_signature::Signature;
use spr_indexer_api::{Context, PaginationToken};
use spr_keypair::X25519Keypair;
use zolana_ring_client::{history::HistoryPosition, KnownUtxo, UtxoOwner};

use crate::{
    frame_repairs::RepairAttempt,
    owner_notes::{IndexTags, OwnerNoteWrites},
    projection_lease::ProjectionLease,
    row_cipher::RowCipher,
    spend_machine::{SpendRow, SpendState},
    tracking::{HeldEnrollment, TrackingCursor},
    utxos::{CreatedIn, LateOwner, PageDelta, StoredUtxo, MAX_LATE_OWNERS},
};
use ring_service_core::{clock::Clock, sql};

const NOTE_SEAL_INFO: &[u8] = b"zolana/ring-rpc-note-seal";
const NOTE_BODY_LEN: usize =
    32 + 1 + 32 + 32 + 1 + 32 + 8 + 32 + 1 + 32 + 32 + 2 + 1 + 8 + 64 + 2 + 4 + 8 + 1 + 32;

/// Keys the note rows of one ring, a stolen database opens no note.
pub(crate) struct NoteSeal {
    cipher: RowCipher,
    ring: Address,
    pub(crate) tags: IndexTags,
}

/// The durable resume point, the history slot moves only at a terminal page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Checkpoint {
    pub position: HistoryPosition,
    pub through: Context,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct LoadedProjection {
    pub spends: LoadedSpends,
    pub tracking_cursor: TrackingCursor,
    /// The snapshot's revision, every row the load read was written at or below it.
    pub revision: i64,
}

/// No note or spend row, the bounded mirror fills from lookups.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct LoadedSpends {
    pub checkpoint: Checkpoint,
    pub first_unread: Option<CreatedIn>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CommittedPage {
    pub revision: i64,
    pub first_unread: Option<CreatedIn>,
}

pub(crate) async fn first_unread(
    connection: &impl ConnectionTrait,
    ring: Address,
) -> Result<Option<CreatedIn>, DbErr> {
    sql::Query::new(
        "SELECT slot,tx_signature,event_index FROM spend_unread_frames WHERE ring=$1 \
         ORDER BY slot,tx_signature,event_index LIMIT 1",
        vec![sql::bytes(ring.as_ref())],
    )
    .one(connection)
    .await?
    .as_ref()
    .map(read_frame)
    .transpose()
}

impl NoteSeal {
    pub(crate) fn derive(auditor: &X25519Keypair, ring: Address) -> Result<Self, DbErr> {
        let mut key = zeroize::Zeroizing::new([0u8; 32]);
        hkdf::Hkdf::<Sha256>::new(None, auditor.secret_bytes().as_slice())
            .expand(NOTE_SEAL_INFO, key.as_mut_slice())
            .map_err(|_| DbErr::Custom("note seal key is unusable".into()))?;
        Ok(Self {
            cipher: RowCipher::new(&key),
            ring,
            tags: IndexTags::derive(auditor, ring),
        })
    }

    pub(crate) fn seal(&self, row_key: &[u8; 32], body: &[u8]) -> Result<Vec<u8>, DbErr> {
        self.cipher
            .seal(&self.aad(row_key), body)
            .map_err(|_| DbErr::Custom("note row failed to seal".into()))
    }

    pub(crate) fn open(&self, row_key: &[u8; 32], sealed: &[u8]) -> Result<Vec<u8>, DbErr> {
        self.cipher
            .open(&self.aad(row_key), sealed)
            .map_err(|_| DbErr::Custom("note row does not open under the auditor key".into()))
    }

    /// The ring and the row key bind the body, a row moved to another key fails to open.
    fn aad(&self, row_key: &[u8; 32]) -> Vec<u8> {
        [self.ring.as_ref(), row_key.as_slice()].concat()
    }
}

/// The one layout of a sealed note.
pub(crate) fn note_body(note: &StoredUtxo) -> Vec<u8> {
    let utxo = &note.utxo;
    // The data fields hold the record's proven hashes under either owner kind.
    let (owner_kind, nullifier_pk) = match utxo.owner {
        UtxoOwner::Keyed { nullifier_pk } => (0, nullifier_pk),
        UtxoOwner::Unresolved => (1, [0; 32]),
    };
    let data = utxo.data_hash.zip(utxo.ring_data_hash);
    let mint = utxo.mint.map_or([0; 32], |mint| mint.to_bytes());
    let nullifier = note.nullifier.unwrap_or_default();
    let (data_hash, ring_data_hash) = data.unwrap_or_default();
    [
        utxo.owner_key.as_slice(),
        &[owner_kind],
        &nullifier_pk,
        &utxo.asset,
        &[u8::from(utxo.mint.is_some())],
        &mint,
        &utxo.amount.to_be_bytes(),
        &utxo.blinding,
        &[u8::from(data.is_some())],
        &data_hash,
        &ring_data_hash,
        &utxo.tree_id.to_be_bytes(),
        &[u8::from(note.ring_note)],
        &note.created_in.slot.to_be_bytes(),
        note.created_in.tx_signature.as_ref(),
        &note.created_in.event_index.to_be_bytes(),
        &note.slot_index.to_be_bytes(),
        &note.leaf_index.to_be_bytes(),
        &[u8::from(note.nullifier.is_some())],
        &nullifier,
    ]
    .concat()
}

pub(crate) fn note_from_body(body: &[u8]) -> Result<StoredUtxo, DbErr> {
    let malformed = || DbErr::Custom("note body has another layout".into());
    if body.len() != NOTE_BODY_LEN {
        return Err(malformed());
    }
    let mut rest = body;
    let mut take = |len: usize| {
        let (taken, tail) = rest.split_at(len);
        rest = tail;
        taken
    };
    fn array<const N: usize>(bytes: &[u8]) -> [u8; N] {
        let mut out = [0; N];
        out.copy_from_slice(bytes);
        out
    }
    let owner_key = array(take(32));
    let owner_kind = take(1)[0];
    let owner_field: [u8; 32] = array(take(32));
    let asset = array(take(32));
    let has_mint = take(1)[0] == 1;
    let mint_bytes: [u8; 32] = array(take(32));
    let amount = u64::from_be_bytes(array(take(8)));
    let blinding = array(take(32));
    let has_data = take(1)[0] == 1;
    let data_hash: [u8; 32] = array(take(32));
    let ring_data_hash: [u8; 32] = array(take(32));
    let owner = match owner_kind {
        0 => UtxoOwner::Keyed {
            nullifier_pk: owner_field,
        },
        1 if owner_field == [0; 32] => UtxoOwner::Unresolved,
        _ => return Err(malformed()),
    };
    let tree_id = u16::from_be_bytes(array(take(2)));
    let ring_note = take(1)[0] == 1;
    let slot = u64::from_be_bytes(array(take(8)));
    let tx_signature = Signature::from(array::<64>(take(64)));
    let event_index = u16::from_be_bytes(array(take(2)));
    let slot_index = u32::from_be_bytes(array(take(4)));
    let leaf_index = u64::from_be_bytes(array(take(8)));
    let has_nullifier = take(1)[0] == 1;
    let nullifier: [u8; 32] = array(take(32));
    Ok(StoredUtxo {
        utxo: KnownUtxo {
            owner_key,
            owner,
            asset,
            mint: has_mint.then(|| Address::new_from_array(mint_bytes)),
            amount,
            blinding,
            data_hash: has_data.then_some(data_hash),
            ring_data_hash: has_data.then_some(ring_data_hash),
            tree_id,
        },
        created_in: CreatedIn {
            slot,
            tx_signature,
            event_index,
        },
        slot_index,
        leaf_index,
        ring_note,
        nullifier: has_nullifier.then_some(nullifier),
    })
}

pub(crate) fn state_name(state: SpendState) -> &'static str {
    match state {
        SpendState::Observed => "observed",
        SpendState::Padding => "padding",
        SpendState::Resolved { .. } => "resolved",
        SpendState::Unclassified => "unclassified",
    }
}

fn state_of(name: &str, link: Option<[u8; 32]>) -> Result<SpendState, DbErr> {
    match (name, link) {
        ("observed", None) => Ok(SpendState::Observed),
        ("padding", None) => Ok(SpendState::Padding),
        ("resolved", Some(note)) => Ok(SpendState::Resolved { note }),
        ("unclassified", None) => Ok(SpendState::Unclassified),
        _ => Err(DbErr::Custom(format!("unknown spend state {name}"))),
    }
}

fn state_link(state: SpendState) -> Option<[u8; 32]> {
    match state {
        SpendState::Resolved { note } => Some(note),
        SpendState::Unclassified | SpendState::Observed | SpendState::Padding => None,
    }
}

fn frame_values(ring: &Address, frame: &CreatedIn) -> Result<Vec<Value>, DbErr> {
    Ok(vec![
        sql::bytes(ring.as_ref()),
        sql::integer(frame.slot)?,
        sql::bytes(frame.tx_signature.as_ref()),
        i32::from(frame.event_index).into(),
    ])
}

pub(crate) fn read_frame(row: &QueryResult) -> Result<CreatedIn, DbErr> {
    Ok(CreatedIn {
        slot: sql::read_integer(row, "slot")?,
        tx_signature: Signature::from(sql::read_bytes::<64>(row, "tx_signature")?),
        event_index: u16::try_from(row.try_get::<i32>("", "event_index")?)
            .map_err(|_| DbErr::Custom("spend event index out of range".into()))?,
    })
}

pub(crate) fn read_repair_attempt(row: &QueryResult) -> Result<RepairAttempt, DbErr> {
    Ok(RepairAttempt {
        attempts: u32::try_from(sql::read_integer(row, "attempts")?)
            .map_err(|_| DbErr::Custom("repair attempts out of range".into()))?,
        retry_after: sql::read_integer(row, "retry_after")?,
    })
}

fn position_values(position: &HistoryPosition) -> Result<[Value; 2], DbErr> {
    Ok([
        position
            .checkpoint()
            .map(sql::integer)
            .transpose()?
            .unwrap_or(Value::BigInt(None)),
        Value::String(position.token().map(|token| Box::new(token.to_string()))),
    ])
}

/// A token resumes inside a slot, `after_slot` alone resumes past a terminal page.
fn read_position(row: &QueryResult) -> Result<HistoryPosition, DbErr> {
    Ok(match row.try_get::<Option<String>>("", "token")? {
        Some(token) => HistoryPosition::resume(
            token
                .parse::<PaginationToken>()
                .map_err(|_| DbErr::Custom("checkpoint token does not parse".into()))?,
        ),
        None => sql::read_optional_integer(row, "after_slot")?
            .map_or_else(HistoryPosition::default, HistoryPosition::after_slot),
    })
}

/// The one layout of a sealed enrollment, the seal last.
fn enrollment_body(held: &HeldEnrollment) -> Vec<u8> {
    [
        held.owner_key.as_slice(),
        &held.owner_hash,
        &held.member,
        &held.nullifier_pk,
        &held.list_tag,
        &held.eph_u,
        &held.list_key,
        &held.ciphertext,
    ]
    .concat()
}

fn enrollment_from_body(body: &[u8]) -> Result<HeldEnrollment, DbErr> {
    let malformed = || DbErr::Custom("enrollment body has another layout".into());
    let (owner_key, rest) = body.split_first_chunk::<32>().ok_or_else(malformed)?;
    let (owner_hash, rest) = rest.split_first_chunk::<32>().ok_or_else(malformed)?;
    let (member, rest) = rest.split_first_chunk::<32>().ok_or_else(malformed)?;
    let (nullifier_pk, rest) = rest.split_first_chunk::<32>().ok_or_else(malformed)?;
    let (list_tag, rest) = rest.split_first_chunk::<32>().ok_or_else(malformed)?;
    let (eph_u, rest) = rest.split_first_chunk::<32>().ok_or_else(malformed)?;
    let (list_key, rest) = rest.split_first_chunk::<32>().ok_or_else(malformed)?;
    let ciphertext: [u8; REGISTER_KEY_CIPHERTEXT_LEN] = rest.try_into().map_err(|_| malformed())?;
    Ok(HeldEnrollment {
        owner_key: *owner_key,
        owner_hash: *owner_hash,
        member: *member,
        nullifier_pk: *nullifier_pk,
        list_tag: *list_tag,
        eph_u: *eph_u,
        ciphertext,
        list_key: *list_key,
    })
}

/// The lease holder's writes, each transaction fenced by the lease before its first row.
pub(crate) struct ProjectionWriter<'a> {
    pub db: &'a DatabaseConnection,
    pub ring: Address,
    pub seal: &'a NoteSeal,
    pub lease: ProjectionLease,
    pub clock: Clock,
}

impl ProjectionWriter<'_> {
    /// The revision row lock orders the writer's transactions, so a follower that read revision
    /// `r` holds every row written at or below it.
    async fn begin(&self) -> Result<(DatabaseTransaction, i64), DbErr> {
        let now = self
            .clock
            .now()
            .map_err(|_| DbErr::Custom("the clock is unreadable".into()))?;
        let transaction = self.db.begin().await?;
        self.lease.fence(&transaction, now).await?;
        sql::Query::new(
            "INSERT INTO projection_revisions(ring,revision) VALUES($1,1) ON CONFLICT(ring) \
             DO UPDATE SET revision=projection_revisions.revision+1",
            vec![self.ring_value()],
        )
        .execute(&transaction)
        .await?;
        let revision = read_revision(&transaction, &self.ring).await?;
        Ok((transaction, revision))
    }

    fn ring_value(&self) -> Value {
        sql::bytes(self.ring.as_ref())
    }

    pub(crate) async fn commit_page(&self, delta: &PageDelta) -> Result<CommittedPage, DbErr> {
        let Self { ring, seal, .. } = self;
        let (transaction, revision) = self.begin().await?;
        for (commitment, note) in delta.notes() {
            let sealed = seal.seal(commitment, &note_body(note))?;
            let nullifier_tag = note
                .nullifier
                .map(|nullifier| seal.tags.nullifier_tag(&nullifier))
                .transpose()?;
            sql::Query::new(
                "INSERT INTO finalized_notes(ring,commitment,sealed,nullifier_tag,revision) \
                 VALUES($1,$2,$3,$4,$5) ON CONFLICT(ring,commitment) DO UPDATE SET \
                 sealed=excluded.sealed,nullifier_tag=excluded.nullifier_tag, \
                 revision=excluded.revision",
                vec![
                    self.ring_value(),
                    sql::bytes(commitment),
                    sealed.into(),
                    Value::Bytes(nullifier_tag.map(|tag| Box::new(tag.to_vec()))),
                    revision.into(),
                ],
            )
            .execute(&transaction)
            .await?;
        }
        for row in delta.rows() {
            let link = state_link(row.state)
                .map(|note| seal.seal(&row.nullifier, &note))
                .transpose()?;
            let mut values = vec![self.ring_value(), sql::bytes(&row.nullifier)];
            values.extend([
                sql::integer(row.frame.slot)?,
                sql::bytes(row.frame.tx_signature.as_ref()),
                i32::from(row.frame.event_index).into(),
                i32::from(row.position).into(),
                state_name(row.state).into(),
                Value::Bytes(link.map(Box::new)),
                revision.into(),
            ]);
            sql::Query::new(
                "INSERT INTO finalized_spent_nullifiers(ring,nullifier,slot,tx_signature, \
                 event_index,position,state,sealed_note,revision) \
                 VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT(ring,nullifier) DO UPDATE SET \
                 state=excluded.state,sealed_note=excluded.sealed_note, \
                 revision=excluded.revision",
                values,
            )
            .execute(&transaction)
            .await?;
        }
        for fault in delta.faults() {
            let mut values = frame_values(ring, &fault.frame)?;
            values.push(sql::bytes(&fault.nullifier));
            sql::Query::new(
                "INSERT INTO spend_integrity_faults(ring,slot,tx_signature,event_index, \
                 nullifier) VALUES($1,$2,$3,$4,$5) \
                 ON CONFLICT(ring,nullifier,tx_signature,event_index) DO NOTHING",
                values,
            )
            .execute(&transaction)
            .await?;
        }
        for frame in delta.read_frames() {
            sql::Query::new(
                "DELETE FROM spend_unread_frames WHERE ring=$1 AND tx_signature=$2 \
                 AND event_index=$3",
                vec![
                    self.ring_value(),
                    sql::bytes(frame.tx_signature.as_ref()),
                    i32::from(frame.event_index).into(),
                ],
            )
            .execute(&transaction)
            .await?;
        }
        // A frame unread again keeps its attempt count, the repair schedule owns it.
        for frame in delta.unread_frames() {
            sql::Query::new(
                "INSERT INTO spend_unread_frames(ring,slot,tx_signature,event_index) \
                 VALUES($1,$2,$3,$4) ON CONFLICT(ring,tx_signature,event_index) DO NOTHING",
                frame_values(ring, frame)?,
            )
            .execute(&transaction)
            .await?;
        }
        OwnerNoteWrites {
            ring: *ring,
            seal,
            delta,
        }
        .write(&transaction)
        .await?;
        if let Some(checkpoint) = delta.checkpoint() {
            let [after_slot, token] = position_values(&checkpoint.position)?;
            sql::Query::new(
                "INSERT INTO spend_checkpoints(ring,after_slot,token,through_slot,block_time) \
                 VALUES($1,$2,$3,$4,$5) ON CONFLICT(ring) DO UPDATE SET \
                 after_slot=excluded.after_slot,token=excluded.token, \
                 through_slot=excluded.through_slot,block_time=excluded.block_time",
                vec![
                    self.ring_value(),
                    after_slot,
                    token,
                    sql::integer(checkpoint.through.slot)?,
                    checkpoint.through.block_time.into(),
                ],
            )
            .execute(&transaction)
            .await?;
        }
        let first_unread = first_unread(&transaction, *ring).await?;
        transaction.commit().await?;
        Ok(CommittedPage {
            revision,
            first_unread,
        })
    }

    /// A page's enrollments and the tracking cursor past it commit together.
    pub(crate) async fn commit_enrollments(
        &self,
        held: &[HeldEnrollment],
        cursor: TrackingCursor,
    ) -> Result<(), DbErr> {
        let (transaction, revision) = self.begin().await?;
        for enrollment in held {
            let member = enrollment.member();
            let sealed = self.seal.seal(&member, &enrollment_body(enrollment))?;
            let inserted = sql::Query::new(
                "INSERT INTO held_enrollments(ring,member,sealed,owner_hash_tag,list_key_tag, \
                 revision) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(ring,member) DO NOTHING",
                vec![
                    self.ring_value(),
                    sql::bytes(&member),
                    sealed.into(),
                    sql::bytes(&self.seal.tags.owner_hash_tag(&enrollment.owner_hash)?),
                    sql::bytes(&self.seal.tags.list_key_tag(&enrollment.list_key)?),
                    revision.into(),
                ],
            )
            .execute(&transaction)
            .await?;
            if inserted != 0 {
                sql::Query::new(
                    "INSERT INTO pending_derivations(ring,member) VALUES($1,$2) \
                     ON CONFLICT(ring,member) DO NOTHING",
                    vec![self.ring_value(), sql::bytes(&member)],
                )
                .execute(&transaction)
                .await?;
            }
        }
        sql::Query::new(
            "INSERT INTO tracking_cursors(ring,next_leaf) VALUES($1,$2) \
             ON CONFLICT(ring) DO UPDATE SET next_leaf=excluded.next_leaf",
            vec![self.ring_value(), sql::integer(cursor.next_leaf())?],
        )
        .execute(&transaction)
        .await?;
        transaction.commit().await
    }

    pub(crate) async fn pending_owners(&self) -> Result<Vec<LateOwner>, DbErr> {
        sql::Query::new(
            "SELECT e.member,e.sealed,p.after_tag FROM held_enrollments e JOIN \
             pending_derivations p ON e.ring=p.ring AND e.member=p.member WHERE e.ring=$1 \
             ORDER BY e.member LIMIT $2",
            vec![self.ring_value(), sql::count(MAX_LATE_OWNERS)?],
        )
        .all(self.db)
        .await?
        .iter()
        .map(|row| {
            let resume = row
                .try_get::<Option<Vec<u8>>>("", "after_tag")?
                .map(<[u8; 32]>::try_from)
                .transpose()
                .map_err(|_| DbErr::Custom("resume tag has another length".into()))?;
            Ok(LateOwner {
                owner: read_enrollment(row, self.seal)?.owner_key,
                resume,
            })
        })
        .collect()
    }

    pub(crate) async fn record_attempts(
        &self,
        attempts: &[(CreatedIn, RepairAttempt)],
    ) -> Result<(), DbErr> {
        let (transaction, _) = self.begin().await?;
        for (frame, attempt) in attempts {
            sql::Query::new(
                "UPDATE spend_unread_frames SET attempts=$1,retry_after=$2 WHERE ring=$3 \
                 AND tx_signature=$4 AND event_index=$5",
                vec![
                    i64::from(attempt.attempts).into(),
                    sql::integer(attempt.retry_after)?,
                    self.ring_value(),
                    sql::bytes(frame.tx_signature.as_ref()),
                    i32::from(frame.event_index).into(),
                ],
            )
            .execute(&transaction)
            .await?;
        }
        transaction.commit().await
    }
}

/// Reads one ring's resume state from one snapshot, a commit between two reads shows in neither.
#[must_use]
pub(crate) struct LoadSpends<'a> {
    pub db: &'a DatabaseConnection,
    pub ring: Address,
}

impl LoadSpends<'_> {
    pub(crate) async fn load(self) -> Result<LoadedProjection, DbErr> {
        let snapshot = read_snapshot(self.db).await?;
        let loaded = self.read(&snapshot).await?;
        snapshot.commit().await?;
        Ok(loaded)
    }

    async fn read(&self, snapshot: &DatabaseTransaction) -> Result<LoadedProjection, DbErr> {
        let ring = &self.ring;
        let query = |select: &str| sql::Query::new(select, vec![sql::bytes(ring.as_ref())]);
        let mut loaded = LoadedProjection {
            revision: read_revision(snapshot, ring).await?,
            ..LoadedProjection::default()
        };
        loaded.spends.first_unread = first_unread(snapshot, *ring).await?;
        if let Some(row) = query(
            "SELECT after_slot,token,through_slot,block_time FROM spend_checkpoints \
             WHERE ring=$1",
        )
        .one(snapshot)
        .await?
        {
            loaded.spends.checkpoint = Checkpoint {
                position: read_position(&row)?,
                through: Context {
                    slot: sql::read_integer(&row, "through_slot")?,
                    block_time: row.try_get("", "block_time")?,
                },
            };
        }
        if let Some(row) = query("SELECT next_leaf FROM tracking_cursors WHERE ring=$1")
            .one(snapshot)
            .await?
        {
            loaded.tracking_cursor = TrackingCursor::stored(sql::read_integer(&row, "next_leaf")?)
                .ok_or_else(|| DbErr::Custom("tracking cursor names the sentinel".into()))?;
        }
        Ok(loaded)
    }
}

pub(crate) const NOTE_COLUMNS: &str = "commitment,sealed";
pub(crate) const ENROLLMENT_COLUMNS: &str = "member,sealed";

/// One enrollment row selected with `ENROLLMENT_COLUMNS`, its body bound to its member.
pub(crate) fn read_enrollment(row: &QueryResult, seal: &NoteSeal) -> Result<HeldEnrollment, DbErr> {
    let member: [u8; 32] = sql::read_bytes(row, "member")?;
    let body = seal.open(&member, &row.try_get::<Vec<u8>>("", "sealed")?)?;
    let held = enrollment_from_body(&body)?;
    if held.member() != member {
        return Err(DbErr::Custom("enrollment row names another member".into()));
    }
    Ok(held)
}
pub(crate) const ROW_COLUMNS: &str =
    "nullifier,slot,tx_signature,event_index,position,state,sealed_note";

/// One note row selected with `NOTE_COLUMNS`.
pub(crate) fn read_note(
    row: &QueryResult,
    seal: &NoteSeal,
) -> Result<([u8; 32], StoredUtxo), DbErr> {
    let commitment: [u8; 32] = sql::read_bytes(row, "commitment")?;
    let body = seal.open(&commitment, &row.try_get::<Vec<u8>>("", "sealed")?)?;
    Ok((commitment, note_from_body(&body)?))
}

/// One spend row selected with `ROW_COLUMNS`, its resolved note link opened under the seal.
pub(crate) fn read_spend_row(row: &QueryResult, seal: &NoteSeal) -> Result<SpendRow, DbErr> {
    let nullifier: [u8; 32] = sql::read_bytes(row, "nullifier")?;
    let link = row
        .try_get::<Option<Vec<u8>>>("", "sealed_note")?
        .map(|sealed| {
            let opened = seal.open(&nullifier, &sealed)?;
            <[u8; 32]>::try_from(opened)
                .map_err(|_| DbErr::Custom("spend link has another length".into()))
        })
        .transpose()?;
    Ok(SpendRow {
        nullifier,
        frame: read_frame(row)?,
        position: u16::try_from(row.try_get::<i32>("", "position")?)
            .map_err(|_| DbErr::Custom("spend position out of range".into()))?,
        state: state_of(&row.try_get::<String>("", "state")?, link)?,
    })
}

/// Zero before the ring's first commit.
pub(crate) async fn read_revision(db: &impl ConnectionTrait, ring: &Address) -> Result<i64, DbErr> {
    let row = sql::Query::new(
        "SELECT revision FROM projection_revisions WHERE ring=$1",
        vec![sql::bytes(ring.as_ref())],
    )
    .one(db)
    .await?;
    row.map_or(Ok(0), |row| row.try_get("", "revision"))
}

/// Postgres pins one snapshot for the transaction, a SQLite read transaction pins it at its
/// first read.
pub(crate) async fn read_snapshot(db: &DatabaseConnection) -> Result<DatabaseTransaction, DbErr> {
    match db.get_database_backend() {
        DatabaseBackend::Postgres => {
            db.begin_with_config(
                Some(IsolationLevel::RepeatableRead),
                Some(AccessMode::ReadOnly),
            )
            .await
        }
        DatabaseBackend::Sqlite | DatabaseBackend::MySql => db.begin().await,
    }
}

#[cfg(test)]
#[path = "projection_tests.rs"]
mod projection_tests;
