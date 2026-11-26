//! The addresses and host values of the storage program accounts.

use bytemuck::Zeroable;
use custom_ring_interface::{
    storage::{RegistryId, StorageRegistry},
    storage_instruction::InitializeTrackingIxData,
    tracking::TrackingRegistry,
    OwnerGrant, ReaderKeyBytes, STORAGE_PROGRAM_ID,
};
use solana_address::Address;

pub trait OwnerGrantHost {
    fn find_address(registry: &Address, owner: &Address, reader: &ReaderKeyBytes) -> (Address, u8);

    fn is_active(&self) -> bool;
}

impl OwnerGrantHost for OwnerGrant {
    fn find_address(registry: &Address, owner: &Address, reader: &ReaderKeyBytes) -> (Address, u8) {
        Address::find_program_address(&Self::seeds(registry, owner, reader), &STORAGE_PROGRAM_ID)
    }

    fn is_active(&self) -> bool {
        self.active == 1
    }
}

pub trait StorageRegistryAddress {
    fn find_address(creator: &Address, registry_id: &RegistryId) -> (Address, u8);
}

impl StorageRegistryAddress for StorageRegistry {
    fn find_address(creator: &Address, registry_id: &RegistryId) -> (Address, u8) {
        Address::find_program_address(&Self::seeds(creator, registry_id), &STORAGE_PROGRAM_ID)
    }
}

pub trait TrackingRegistryHost: Sized {
    fn find_address(creator: &Address, ring_id: &[u8; 32]) -> (Address, u8);

    /// Holds only the sentinel root at revision zero, host only as the value overflows the SBF stack frame.
    fn new(authority: Address, ix: &InitializeTrackingIxData) -> Self;
}

impl TrackingRegistryHost for TrackingRegistry {
    fn find_address(creator: &Address, ring_id: &[u8; 32]) -> (Address, u8) {
        Address::find_program_address(&Self::seeds(creator, ring_id), &STORAGE_PROGRAM_ID)
    }

    fn new(authority: Address, ix: &InitializeTrackingIxData) -> Self {
        let mut registry = Self::zeroed();
        registry.init(authority, ix);
        registry
    }
}
