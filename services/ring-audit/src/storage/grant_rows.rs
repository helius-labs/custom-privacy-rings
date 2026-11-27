use hkdf::hmac::{Hmac, Mac};
use sea_orm::Value;
use sha2::Sha256;
use solana_address::Address;
use spr_keypair::KeypairError;
use zeroize::Zeroizing;
use zolana_ring_client::{rpc::ReaderGrantAction, ReaderKey};

use ring_service_core::{database::Store, keys::GRANT_ROW_KEY_INFO, sql};

use crate::{error::RingAuditError, grants::GrantQuery, row_cipher::RowCipher};

const ACTION_AND_TIMESTAMP_LEN: usize = 1 + 8;

pub struct GrantRows {
    store: Store,
    key: GrantRowKey,
}

pub(crate) struct GrantRowKey {
    cipher: RowCipher,
    lookup: Zeroizing<[u8; 32]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GrantPair {
    pub ring: Address,
    /// `None` for a ring-wide grant.
    pub wallet: Option<Address>,
    pub reader: ReaderKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GrantRow {
    pub pair: GrantPair,
    pub action: ReaderGrantAction,
    /// The signed Unix seconds, the newest row of a pair holds.
    pub timestamp: u64,
}

impl GrantRowKey {
    pub(crate) fn derive(secret: &[u8]) -> Result<Self, RingAuditError> {
        let mut okm = Zeroizing::new([0u8; 64]);
        hkdf::Hkdf::<Sha256>::new(None, secret)
            .expand(GRANT_ROW_KEY_INFO, okm.as_mut_slice())
            .map_err(|_| KeypairError::Hkdf)?;
        // Disjoint halves, the cipher key never keys the lookup.
        let (cipher, lookup) = okm.split_first_chunk().ok_or(KeypairError::Hkdf)?;
        Ok(Self {
            cipher: RowCipher::new(cipher),
            lookup: Zeroizing::new(lookup.try_into().map_err(|_| KeypairError::Hkdf)?),
        })
    }

    /// The only plaintext a row stores beside its insert id.
    fn lookup(&self, reader: &ReaderKey) -> Result<[u8; 32], RingAuditError> {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(self.lookup.as_slice())
            .map_err(|_| KeypairError::Hkdf)?;
        mac.update(&reader.to_bytes());
        Ok(mac.finalize().into_bytes().into())
    }
}

impl GrantRow {
    /// A ring-wide row leaves the wallet out.
    fn body(&self) -> Vec<u8> {
        let action = match self.action {
            ReaderGrantAction::Grant => 1,
            ReaderGrantAction::Revoke => 0,
        };
        [
            self.pair.ring.as_ref(),
            self.pair.wallet.as_ref().map_or(&[][..], AsRef::as_ref),
            &[action],
            &self.timestamp.to_le_bytes(),
        ]
        .concat()
    }

    fn from_body(reader: ReaderKey, body: &[u8]) -> Result<Self, RingAuditError> {
        let invalid = || RingAuditError::InvalidGrantRow;
        let (ring, rest) = body.split_first_chunk::<32>().ok_or_else(invalid)?;
        let (wallet, tail) = rest
            .split_last_chunk::<ACTION_AND_TIMESTAMP_LEN>()
            .ok_or_else(invalid)?;
        let wallet = match wallet {
            [] => None,
            wallet => Some(Address::new_from_array(
                wallet.try_into().map_err(|_| invalid())?,
            )),
        };
        let (action, timestamp) = tail.split_first().ok_or_else(invalid)?;
        Ok(Self {
            pair: GrantPair {
                ring: Address::new_from_array(*ring),
                wallet,
                reader,
            },
            action: match action {
                1 => ReaderGrantAction::Grant,
                0 => ReaderGrantAction::Revoke,
                _ => return Err(invalid()),
            },
            timestamp: u64::from_le_bytes(timestamp.try_into().map_err(|_| invalid())?),
        })
    }
}

impl GrantRows {
    pub(crate) fn new(store: Store, key: GrantRowKey) -> Self {
        Self { store, key }
    }

    /// Appends the row, a stale signature leaves the newer action in force.
    pub(crate) async fn write(&self, row: GrantRow) -> Result<ReaderGrantAction, RingAuditError> {
        let lookup = self.key.lookup(&row.pair.reader)?;
        // The lookup is the associated data, a row moved to another reader fails to open.
        let sealed = self
            .key
            .cipher
            .seal(&lookup, &row.body())
            .map_err(|_| RingAuditError::InvalidGrantRow)?;
        let db = self.store.connection();
        sql::Query::new(
            "INSERT INTO grants (reader, sealed) VALUES ($1, $2)",
            vec![sql::bytes(&lookup), Value::from(sealed)],
        )
        .execute(db)
        .await?;
        let rows = self.rows(row.pair.reader).await?;
        Ok(newest(&rows, row.pair).map_or(row.action, |newest| newest.action))
    }

    pub(crate) async fn grants(&self, query: GrantQuery) -> Result<bool, RingAuditError> {
        let rows = self.rows(query.reader).await?;
        let granted = |wallet| {
            let pair = GrantPair {
                ring: query.ring,
                wallet,
                reader: query.reader,
            };
            newest(&rows, pair).is_some_and(|row| row.action == ReaderGrantAction::Grant)
        };
        Ok(granted(None) || query.wallet.is_some_and(|wallet| granted(Some(wallet))))
    }

    /// In insert order.
    async fn rows(&self, reader: ReaderKey) -> Result<Vec<GrantRow>, RingAuditError> {
        let lookup = self.key.lookup(&reader)?;
        let db = self.store.connection();
        let rows = sql::Query::new(
            "SELECT sealed FROM grants WHERE reader = $1 ORDER BY id",
            vec![sql::bytes(&lookup)],
        )
        .all(db)
        .await?;
        rows.iter()
            .map(|row| {
                let sealed: Vec<u8> = row.try_get("", "sealed")?;
                let body = self
                    .key
                    .cipher
                    .open(&lookup, &sealed)
                    .map_err(|_| RingAuditError::InvalidGrantRow)?;
                GrantRow::from_body(reader, &body)
            })
            .collect()
    }
}

/// The pair's row with the latest timestamp, the later insert on a tie.
fn newest(rows: &[GrantRow], pair: GrantPair) -> Option<GrantRow> {
    rows.iter()
        .filter(|row| row.pair == pair)
        .max_by_key(|row| row.timestamp)
        .copied()
}

#[cfg(test)]
mod tests {
    use solana_signer::Signer;
    use spr_keypair::X25519Keypair;

    use super::*;
    use crate::spent_index::NoteSeal;

    const SECRET: [u8; 32] = [0x41; 32];
    const RING: Address = Address::new_from_array([8; 32]);
    /// A stored ring-wide grant of `reader(1)` at second 7 under `SECRET`.
    const SEALED_GRANT: [u8; 69] = [
        99, 17, 137, 154, 200, 43, 165, 11, 161, 109, 15, 108, 93, 215, 26, 145, 73, 131, 167, 0,
        172, 201, 153, 97, 195, 217, 174, 0, 195, 110, 216, 29, 66, 129, 111, 177, 3, 204, 98, 151,
        68, 193, 128, 38, 225, 222, 146, 106, 3, 73, 198, 102, 70, 84, 63, 14, 88, 61, 21, 27, 114,
        168, 40, 7, 47, 253, 216, 55, 80,
    ];

    fn reader(seed: u8) -> ReaderKey {
        ReaderKey::ed25519(solana_keypair::Keypair::new_from_array([seed; 32]).pubkey())
            .expect("reader")
    }

    async fn store_row(rows: &GrantRows, reader: ReaderKey, sealed: &[u8]) {
        let db = rows.store.connection();
        sql::Query::new(
            "INSERT INTO grants (reader, sealed) VALUES ($1, $2)",
            vec![
                sql::bytes(&rows.key.lookup(&reader).expect("lookup")),
                sealed.to_vec().into(),
            ],
        )
        .execute(db)
        .await
        .expect("insert");
    }

    #[tokio::test]
    async fn a_stored_grant_row_opens_only_under_its_readers_lookup() {
        let rows = GrantRows::new(
            Store::open("sqlite::memory:", 1).await.expect("store"),
            GrantRowKey::derive(&SECRET).expect("key"),
        );
        store_row(&rows, reader(1), &SEALED_GRANT).await;
        store_row(&rows, reader(2), &SEALED_GRANT).await;
        store_row(&rows, reader(3), &SEALED_GRANT[..11]).await;

        let opened = rows.rows(reader(1)).await.expect("opens");
        assert_eq!(
            opened,
            [GrantRow {
                pair: GrantPair {
                    ring: RING,
                    wallet: None,
                    reader: reader(1),
                },
                action: ReaderGrantAction::Grant,
                timestamp: 7,
            }]
        );
        for moved in [reader(2), reader(3)] {
            assert!(matches!(
                rows.rows(moved).await,
                Err(RingAuditError::InvalidGrantRow)
            ));
        }
    }

    #[test]
    fn grant_and_note_keys_differ_under_one_secret() {
        let grants = GrantRowKey::derive(&SECRET).expect("key");
        let notes = NoteSeal::derive(&X25519Keypair::from_bytes(&SECRET), RING).expect("seal");
        let row_key = [0x22; 32];
        let sealed = notes.seal(&row_key, b"row").expect("sealed");
        let aad = [RING.as_ref(), &row_key].concat();

        assert_eq!(notes.open(&row_key, &sealed).expect("opens"), b"row");
        assert!(grants.cipher.open(&aad, &sealed).is_err());
    }
}
