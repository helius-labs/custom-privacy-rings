use solana_address::Address;
use thiserror::Error;

use ring_writer::storage_chain::{ring_policy, storage_registry, StorageChain};

use crate::error::RingRelayError;

#[must_use]
pub struct RolePins {
    pub ring: Address,
    pub storage_authority: Option<Address>,
    pub enrollment_writer: Option<Address>,
}

#[derive(Debug, Error)]
pub enum RolePinError {
    #[error("the ring names enrollment writer {held}, the custody manifest pins {pinned}")]
    EnrollmentWriter { pinned: Address, held: Address },
    #[error("the ring records name storage authority {held}, the custody manifest pins {pinned}")]
    StorageAuthority { pinned: Address, held: Address },
    #[error("cannot read the ring roles")]
    Chain(#[from] RingRelayError),
}

/// `None` for a role the chain does not hold yet.
struct HeldRoles {
    enrollment_writer: Option<Address>,
    storage_authority: Option<Address>,
}

impl RolePins {
    /// An uninitialized ring or records registry passes, a deployment starts the service ahead of `Initialize`.
    pub async fn check(self, chain: &dyn StorageChain) -> Result<(), RolePinError> {
        let held = HeldRoles::read(chain, self.ring).await?;
        self.matches(&held)
    }

    fn matches(&self, held: &HeldRoles) -> Result<(), RolePinError> {
        if let (Some(pinned), Some(held)) = (self.enrollment_writer, held.enrollment_writer) {
            if pinned != held {
                return Err(RolePinError::EnrollmentWriter { pinned, held });
            }
        }
        if let (Some(pinned), Some(held)) = (self.storage_authority, held.storage_authority) {
            if pinned != held {
                return Err(RolePinError::StorageAuthority { pinned, held });
            }
        }
        Ok(())
    }
}

impl HeldRoles {
    async fn read(chain: &dyn StorageChain, ring: Address) -> Result<Self, RingRelayError> {
        let Some(policy) = ring_policy(chain, ring).await? else {
            return Ok(Self {
                enrollment_writer: None,
                storage_authority: None,
            });
        };
        Ok(Self {
            enrollment_writer: Some(policy.roles.enrollment_writer),
            storage_authority: storage_registry(chain, policy.records)
                .await?
                .map(|registry| registry.authority),
        })
    }
}

#[cfg(any(test, feature = "test-fixtures"))]
pub mod fixture {
    use std::collections::HashMap;

    use async_trait::async_trait;
    use custom_ring_interface::{storage::StorageRegistry, ApplyRecordsIxData};
    use solana_account::Account;
    use solana_address::Address;
    use zolana_ring_client::TrackingLeaf;
    use zolana_ring_client::{EnrollBatch, ProvenEnrollBatch};
    use zolana_ring_indexer::rpc::{
        GetStorageApplyProofInputsRequest, GetStorageApplyProofInputsResponse,
        GetStorageMemberOpeningsRequest, GetStorageMemberOpeningsResponse,
    };
    use zolana_ring_member_registry::ApplyProofInputs;

    use ring_service_core::{
        landing::{fixture::UnusedChain, ChainSubmit},
        upstream::RingConfiguration,
    };
    use ring_writer::{storage_chain::StorageChain, WriterError};

    use crate::error::ServiceError;

    /// `None` fails every account read.
    pub struct RingAccounts(pub Option<HashMap<Address, Account>>);

    pub fn account(owner: Address, data: &[u8]) -> Account {
        Account {
            lamports: 1,
            data: data.to_vec(),
            owner,
            executable: false,
            rent_epoch: 0,
        }
    }

    #[async_trait]
    impl StorageChain for RingAccounts {
        fn submit(&self) -> &dyn ChainSubmit {
            &UnusedChain
        }

        async fn account(&self, address: Address) -> Result<Option<Account>, WriterError> {
            let accounts = self
                .0
                .as_ref()
                .ok_or(WriterError::Service(ServiceError::StateUnavailable))?;
            Ok(accounts.get(&address).cloned())
        }

        async fn ring_config(
            &self,
            _ring: Address,
        ) -> Result<Option<RingConfiguration>, WriterError> {
            unreachable!("a role check reads the policy account")
        }

        async fn member_openings(
            &self,
            _request: GetStorageMemberOpeningsRequest,
        ) -> Result<GetStorageMemberOpeningsResponse, ServiceError> {
            unreachable!("a role check reads no leaf")
        }

        async fn apply_inputs(
            &self,
            _request: GetStorageApplyProofInputsRequest,
        ) -> Result<GetStorageApplyProofInputsResponse, WriterError> {
            unreachable!("a role check writes no leaf")
        }

        async fn prove_apply(
            &self,
            _inputs: &ApplyProofInputs,
            _registry: &StorageRegistry,
        ) -> Result<ApplyRecordsIxData, WriterError> {
            unreachable!("a role check proves nothing")
        }

        async fn prove_enrollments(
            &self,
            _batch: EnrollBatch<'_>,
        ) -> Result<ProvenEnrollBatch, WriterError> {
            unreachable!("a role check proves nothing")
        }

        async fn tracking_leaves(
            &self,
            _registry: Address,
            _owners: &[[u8; 32]],
        ) -> Result<HashMap<[u8; 32], TrackingLeaf>, WriterError> {
            unreachable!("a role check reads no leaf")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bytemuck::Zeroable;
    use custom_ring_interface::{
        storage::{RegistryId, StorageRegistry},
        storage_instruction::InitializeIxData,
        RingPolicy, RING_POLICY, STORAGE_PROGRAM_ID,
    };
    use zolana_ring_client::CustomRing;

    use super::{
        fixture::{account, RingAccounts},
        *,
    };

    const RING: Address = Address::new_from_array([1; 32]);
    const RECORDS: Address = Address::new_from_array([2; 32]);
    const WRITER: Address = Address::new_from_array([3; 32]);
    const AUTHORITY: Address = Address::new_from_array([4; 32]);
    const OTHER: Address = Address::new_from_array([5; 32]);

    #[derive(Clone, Copy)]
    enum Initialized {
        Nothing,
        Policy,
        PolicyAndRecords,
    }

    /// The ring names WRITER and its records registry names AUTHORITY.
    fn chain(initialized: Initialized) -> RingAccounts {
        let mut policy = RingPolicy::zeroed();
        policy.discriminator = RING_POLICY;
        policy.records = RECORDS;
        policy.roles.enrollment_writer = WRITER;
        let registry = StorageRegistry::new(
            AUTHORITY,
            &InitializeIxData {
                registry_id: RegistryId([0; 32]),
                grace_slots: [0; 8],
                ring_id: [1; 32],
                tracking_registry: Address::new_from_array([6; 32]),
            },
        );
        let policy = (
            CustomRing::new(RING).policy_pda(),
            account(RING, bytemuck::bytes_of(&policy)),
        );
        let records = (
            RECORDS,
            account(STORAGE_PROGRAM_ID, bytemuck::bytes_of(&registry)),
        );
        RingAccounts(Some(match initialized {
            Initialized::Nothing => HashMap::new(),
            Initialized::Policy => HashMap::from([policy]),
            Initialized::PolicyAndRecords => HashMap::from([policy, records]),
        }))
    }

    fn pins(storage_authority: Option<Address>, enrollment_writer: Option<Address>) -> RolePins {
        RolePins {
            ring: RING,
            storage_authority,
            enrollment_writer,
        }
    }

    #[tokio::test]
    async fn a_pin_passes_unless_the_chain_holds_the_role_under_another_address() {
        use Initialized::*;
        let passing = [
            (
                "matching pins",
                PolicyAndRecords,
                Some(AUTHORITY),
                Some(WRITER),
            ),
            ("no pinned role", PolicyAndRecords, None, None),
            (
                "an unpinned writer",
                PolicyAndRecords,
                Some(AUTHORITY),
                None,
            ),
            (
                "an unpinned authority",
                PolicyAndRecords,
                None,
                Some(WRITER),
            ),
            ("an uninitialized ring", Nothing, Some(OTHER), Some(OTHER)),
            (
                "an uninitialized registry",
                Policy,
                Some(OTHER),
                Some(WRITER),
            ),
        ];
        for (case, initialized, storage_authority, enrollment_writer) in passing {
            pins(storage_authority, enrollment_writer)
                .check(&chain(initialized))
                .await
                .expect(case);
        }
    }

    #[tokio::test]
    async fn a_pin_the_chain_contradicts_is_refused_with_both_addresses() {
        let writer = pins(Some(AUTHORITY), Some(OTHER))
            .check(&chain(Initialized::PolicyAndRecords))
            .await;
        assert!(matches!(
            writer,
            Err(RolePinError::EnrollmentWriter {
                pinned: OTHER,
                held: WRITER
            })
        ));
        let writer_ahead_of_records = pins(None, Some(OTHER))
            .check(&chain(Initialized::Policy))
            .await;
        assert!(matches!(
            writer_ahead_of_records,
            Err(RolePinError::EnrollmentWriter { .. })
        ));
        let authority = pins(Some(OTHER), Some(WRITER))
            .check(&chain(Initialized::PolicyAndRecords))
            .await;
        assert!(matches!(
            authority,
            Err(RolePinError::StorageAuthority {
                pinned: OTHER,
                held: AUTHORITY
            })
        ));
    }

    #[tokio::test]
    async fn an_unreadable_chain_fails_the_check() {
        let unreadable = pins(None, None).check(&RingAccounts(None)).await;
        assert!(matches!(unreadable, Err(RolePinError::Chain(_))));
    }
}
