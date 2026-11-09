//! The viewing key and the seal of each tracking leaf version, beside the leaf fields the tree hashes.
//!
//! A leaf commits to `viewing_hash` and `ct_hash` only. Each version keeps the viewing key the hash
//! commits to and the seal of the Enroll write that inserted the leaf, so a served opening carries
//! both and a reader checks them against the leaf.

use anyhow::Result;
use custom_ring_interface::REGISTER_KEY_CIPHERTEXT_LEN;
use sea_orm::{ConnectionTrait, QueryResult};

use super::{bytes, field, int, uint, TrackingStore};

/// The owner and the sealed secrets of the write that inserted a leaf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LeafSeal {
    pub owner: [u8; 32],
    pub eph_u: [u8; 32],
    pub ciphertext: [u8; REGISTER_KEY_CIPHERTEXT_LEN],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LeafKey {
    pub viewing_u: [u8; 32],
    pub seq: u64,
    pub seal: LeafSeal,
}

/// What one applied write left on its owner's leaf.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LeafStamp {
    pub member: [u8; 32],
    pub viewing_u: [u8; 32],
    pub seq: u64,
    /// Set on the insert alone, an update keeps the inserting write's.
    pub seal: Option<LeafSeal>,
}

const KEY_COLUMNS: [&str; 5] = ["viewing_u", "seq", "owner", "eph_u", "ciphertext"];

impl<C: ConnectionTrait> TrackingStore<'_, C> {
    /// Every leaf version saved at `revision` takes its predecessor's key and seal, then each stamp overrides its owner's.
    pub async fn stamp_leaves(&self, revision: u64, stamps: &[LeafStamp]) -> Result<()> {
        let carried = KEY_COLUMNS
            .iter()
            .map(|column| {
                format!(
                    "{column}=(SELECT p.{column} FROM tracking_records p \
                     WHERE p.program=tracking_records.program AND p.member=tracking_records.member \
                     AND p.applied<tracking_records.applied ORDER BY p.applied DESC LIMIT 1)"
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("UPDATE tracking_records SET {carried} WHERE program=$1 AND applied=$2");
        self.tree.exec(&sql, vec![int(revision)?]).await?;
        for stamp in stamps {
            let (sql, mut values) = match &stamp.seal {
                Some(seal) => (
                    "UPDATE tracking_records SET viewing_u=$4, seq=$5, owner=$6, eph_u=$7, ciphertext=$8 \
                     WHERE program=$1 AND applied=$2 AND member=$3",
                    vec![
                        bytes(&seal.owner),
                        bytes(&seal.eph_u),
                        bytes(&seal.ciphertext),
                    ],
                ),
                None => (
                    "UPDATE tracking_records SET viewing_u=$4, seq=$5 \
                     WHERE program=$1 AND applied=$2 AND member=$3",
                    vec![],
                ),
            };
            values.splice(
                0..0,
                [
                    int(revision)?,
                    bytes(&stamp.member),
                    bytes(&stamp.viewing_u),
                    int(stamp.seq)?,
                ],
            );
            self.tree.exec(sql, values).await?;
        }
        Ok(())
    }

    /// The key and seal of `member`'s newest leaf version at or below `version`.
    pub async fn leaf_key(&self, member: &[u8; 32], version: u64) -> Result<Option<LeafKey>> {
        let sql = "SELECT viewing_u, seq, owner, eph_u, ciphertext FROM tracking_records \
                   WHERE program=$1 AND member=$2 AND applied<=$3 ORDER BY applied DESC LIMIT 1";
        let row = self
            .tree
            .query_one(sql, vec![bytes(member), int(version)?])
            .await?;
        row.as_ref().map(leaf_key).transpose()
    }
}

fn leaf_key(row: &QueryResult) -> Result<LeafKey> {
    Ok(LeafKey {
        viewing_u: field(row, "viewing_u")?,
        seq: uint(row, "seq")?,
        seal: LeafSeal {
            owner: field(row, "owner")?,
            eph_u: field(row, "eph_u")?,
            ciphertext: field(row, "ciphertext")?,
        },
    })
}
