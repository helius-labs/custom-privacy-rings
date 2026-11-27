//! Member records read while the projection catches up.

use std::fmt::Display;

use anyhow::{bail, Context as _, Result};
use custom_ring_interface::RingPolicy;
use spr_client::{BlockingIndexer, BlockingRpc};
use spr_indexer_api::Hash;
use zolana_ring_client::{
    live_registry,
    records::{MemberOpeningRead, StorageTip},
    AuthorityRecordUpdate, MemberRecords, RecordUpdateTarget, RingError, RingIndexer,
};
use zolana_ring_indexer::rpc::MAX_RING_MEMBER_PAGE_LIMIT;
use zolana_ring_policy::Member;

use crate::{wait_for, Context, Probe, Zolana};

pub const NO_RING_POLICY: &str =
    "the ring has no ring policy, its authority must run `zolana-ring init` first";

pub(crate) fn read<R: BlockingRpc, I: BlockingIndexer + RingIndexer>(
    zolana: &Zolana<R, I>,
    records: MemberRecords<'_>,
) -> Result<Vec<MemberOpeningRead>> {
    let label = format!("the member records of {}", records.ring.program_id());
    patiently(label, || zolana.block_on(records.read(zolana.as_async())))
}

pub(crate) fn read_one<R: BlockingRpc, I: BlockingIndexer + RingIndexer>(
    zolana: &Zolana<R, I>,
    records: MemberRecords<'_>,
) -> Result<MemberOpeningRead> {
    let [read] =
        <[_; 1]>::try_from(read(zolana, records)?).map_err(|_| RingError::InvalidRecordOpening)?;
    Ok(read)
}

/// `members` under the newest root of the live curator registry.
pub(crate) fn read_curator<R: BlockingRpc, I: BlockingIndexer + RingIndexer>(
    zolana: &Zolana<R, I>,
    policy: &RingPolicy,
    members: &[Member],
) -> Result<Vec<MemberOpeningRead>> {
    let registry = policy.curator().context("the policy names no curator")?;
    let live = zolana.block_on(live_registry(registry, zolana.as_async()))?;
    let tip = StorageTip::newest(registry, &live);
    let members: Vec<[u8; 32]> = members.iter().map(|member| *member.as_bytes()).collect();
    let mut reads = Vec::with_capacity(members.len());
    for chunk in members.chunks(MAX_RING_MEMBER_PAGE_LIMIT as usize) {
        let response = zolana
            .indexer()
            .get_storage_member_openings(tip.openings_request(chunk))?;
        reads.extend(tip.verify_openings(&response, chunk)?);
    }
    Ok(reads)
}

/// Read again under the newest root while the projection lags the chain.
pub fn patiently<T>(
    label: impl Display,
    mut read: impl FnMut() -> Result<T, RingError>,
) -> Result<T> {
    wait_for(label, || match read() {
        Ok(value) => Ok(Probe::Ready(value)),
        Err(error) if error.is_projection_lag() => Ok(Probe::Retry(error.into())),
        Err(RingError::MissingRingPolicy) => bail!(NO_RING_POLICY),
        Err(error) => Err(error.into()),
    })
}

/// All nonzero mint settings, authenticated under the newest root of the ring's records.
pub(crate) fn mint_policies(
    ctx: &Context,
) -> Result<std::collections::BTreeMap<[u8; 32], zolana_ring_policy::MintPolicy>> {
    let label = format!("the mint policies of {}", ctx.ring.program_id());
    patiently(label, || read_mint_policies(ctx))
}

fn read_mint_policies(
    ctx: &Context,
) -> Result<std::collections::BTreeMap<[u8; 32], zolana_ring_policy::MintPolicy>, RingError> {
    use zolana_ring_indexer::rpc::{GetStorageMemberRecordsRequest, RingMemberOpening};
    let ring_policy = ctx.block(ctx.ring.require_policy(ctx.chain()))?;
    let records = ring_policy.records;
    let tip = StorageTip::newest(records, &ctx.block(live_registry(records, ctx.chain()))?);
    let mut result = std::collections::BTreeMap::new();
    let mut start = Some(1);
    while let Some(index) = start {
        let page =
            ctx.zolana
                .indexer()
                .get_storage_member_records(GetStorageMemberRecordsRequest {
                    registry: tip.registry.into(),
                    expected_root: Hash(tip.root),
                    start_index: index,
                    limit: MAX_RING_MEMBER_PAGE_LIMIT,
                })?;
        start = tip.next_page(&page, index)?;
        for row in page.records {
            let member = row.record.member.0;
            let read = MemberOpeningRead::verify(
                &RingMemberOpening {
                    present: true,
                    record: row.record,
                    path: row.path,
                },
                &tip.root,
                &member,
            )?;
            let policy = read
                .record()
                .ok_or(RingError::InvalidRecordOpening)?
                .mint_policy;
            if policy != zolana_ring_policy::MintPolicy::EMPTY {
                result.insert(member, policy);
            }
        }
    }
    Ok(result)
}

/// One authority update carries every mint of `desired`.
pub(crate) fn write_mint_policies(
    ctx: &Context,
    desired: &std::collections::BTreeMap<[u8; 32], zolana_ring_policy::MintPolicy>,
) -> Result<()> {
    if desired.is_empty() {
        return Ok(());
    }
    ctx.apply_authority_update(ctx.record_update_target()?, &[], |update| {
        desired.iter().try_fold(update, |update, (member, policy)| {
            Ok(update.with_mint_policy(Member::from_bytes(*member)?, *policy))
        })
    })?;
    Ok(())
}

impl Context {
    pub(crate) fn apply_authority_update(
        &self,
        target: RecordUpdateTarget,
        unregistered: &[solana_address::Address],
        build: impl FnOnce(AuthorityRecordUpdate) -> Result<AuthorityRecordUpdate>,
    ) -> Result<u64> {
        let authority = self.authority()?;
        let update = build(AuthorityRecordUpdate::new(target, authority.address()))?;
        let signed = update.sign(&self.request_signer(authority.as_ref()))?;
        self.ring_rpc()?
            .apply_record_update(signed.with_unregistered_owners(unregistered))
    }
}

#[cfg(test)]
mod tests {
    use custom_ring_interface::{
        storage::StorageRegistry, storage_instruction::InitializeIxData, STORAGE_PROGRAM_ID,
    };
    use solana_account::Account;
    use solana_address::Address;
    use spr_client::{AsyncProverClient, ClientError, Rpc, WitnessReader};
    use zolana_ring_client::RingIndexerError;
    use zolana_ring_indexer::rpc::{
        GetStorageMemberOpeningsRequest, GetStorageMemberOpeningsResponse, RingMemberOpening,
        RingMemberRecordView, RingRecordPathView, StorageRootView,
    };

    use super::*;
    use crate::ZolanaServices;

    const CURATOR: Address = Address::new_from_array([9; 32]);

    struct Curator(Account);

    impl Rpc for Curator {
        fn get_account(&self, address: Address) -> Result<Option<Account>, ClientError> {
            Ok((address == CURATOR).then(|| self.0.clone()))
        }
    }

    /// Serves `opening` for every member, under the root it opens to.
    struct Indexer {
        opening: RingMemberOpening,
        root: [u8; 32],
    }

    impl WitnessReader for Indexer {}
    impl Rpc for Indexer {}

    impl RingIndexer for Indexer {
        fn get_storage_member_openings(
            &self,
            request: GetStorageMemberOpeningsRequest,
        ) -> Result<GetStorageMemberOpeningsResponse, RingIndexerError> {
            Ok(GetStorageMemberOpeningsResponse {
                context: spr_client::Context {
                    block_time: 0,
                    slot: 1,
                },
                root: StorageRootView {
                    root: Hash(self.root),
                    revision: 1,
                    next_index: 1,
                },
                openings: vec![self.opening.clone(); request.members.len()],
            })
        }
    }

    fn field(json: &serde_json::Value) -> [u8; 32] {
        hex::decode(json.as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap()
    }

    #[test]
    fn a_curator_opening_under_another_root_is_refused() {
        let vector: serde_json::Value = serde_json::from_str(include_str!(
            "../../../test-vectors/registry/ring-records.json"
        ))
        .unwrap();
        let served = &vector["openings"][0];
        let record = &served["record"];
        let path = &served["path"];
        let siblings = path["siblings"].as_array().unwrap().iter().map(|level| {
            let level: Vec<Hash> = level
                .as_array()
                .unwrap()
                .iter()
                .map(field)
                .map(Hash)
                .collect();
            <[Hash; 3]>::try_from(level).unwrap()
        });
        let opening = RingMemberOpening {
            present: served["present"].as_bool().unwrap(),
            record: RingMemberRecordView {
                member: Hash(field(&record["member"])),
                next: Hash(field(&record["next"])),
                lists: record["lists"].as_u64().unwrap(),
                mint_policy: zolana_ring_policy::MintPolicy::EMPTY,
            },
            path: RingRecordPathView {
                index: path["index"].as_u64().unwrap(),
                siblings: siblings.collect(),
            },
        };
        let root = field(&served["root"]);
        let member = Member::from_bytes(opening.record.member.0).unwrap();
        let mut policy: Box<RingPolicy> = bytemuck::zeroed_box();
        policy.sources[0].registry = CURATOR;
        let read = |newest: [u8; 32]| {
            let mut registry = StorageRegistry::new(
                Address::default(),
                &InitializeIxData {
                    registry_id: custom_ring_interface::storage::RegistryId([0; 32]),
                    grace_slots: [0; 8],
                    ring_id: [0; 32],
                    tracking_registry: Address::default(),
                },
            );
            registry.roots[0].hash = newest;
            let zolana = ZolanaServices {
                rpc: Curator(Account {
                    lamports: 1,
                    data: bytemuck::bytes_of(&registry).to_vec(),
                    owner: STORAGE_PROGRAM_ID,
                    executable: false,
                    rent_epoch: 0,
                }),
                indexer: Indexer {
                    opening: opening.clone(),
                    root,
                },
                prover: AsyncProverClient::local(),
                ring_prover: AsyncProverClient::local(),
            }
            .connect();
            read_curator(&zolana, &policy, &[member, member])
        };
        let reads = read(root).unwrap();
        assert_eq!(reads.len(), 2);
        assert!(reads.iter().all(|read| read.record().is_some()));
        let mut stale = root;
        stale[31] ^= 1;
        let Err(refused) = read(stale) else {
            panic!("a stale opening reads");
        };
        assert!(
            matches!(
                refused.downcast_ref(),
                Some(zolana_ring_client::AuditError::InvalidMemberRegistry)
            ),
            "{refused}"
        );
    }
}
