//! A co-signer's approval of one transfer call, for the program, the SDKs and Ring RPC.
//!
//! A signature forwarded through a calling program covers every call that program makes. An
//! `ApproveTransfer` instruction at the top level covers one call, and the transfer reads it from the
//! instructions sysvar in place of the co-signer's signature.
//!
//! The instruction:
//!
//! 1. Accounts `[ring, co_signer_record, co_signer]`, the co-signer signs and nothing is written.
//! 2. Data `[tag::APPROVE_TRANSFER, position, digest]`.
//! 3. One instruction covers one co-signer record.
//!
//! The transfer names `[co_signer_record, instructions_sysvar]` in the pair of each approved record.

use arrayvec::ArrayVec;
use solana_address::Address;
use spr_hasher::{sha256::Sha256, Hasher};

use crate::{cosign::MAX_CO_SIGNERS, tag, CustomRingError, MAX_SPP_CPI_ACCOUNTS};

/// The ring account, three registries and the co-signer pairs lead SPP's list.
pub const MAX_TRANSFER_ACCOUNTS: usize = 1 + 3 + 2 * MAX_CO_SIGNERS + MAX_SPP_CPI_ACCOUNTS;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransferApproval {
    /// Index of the top-level instruction the transfer runs inside.
    pub position: u8,
    pub digest: [u8; 32],
}

impl TransferApproval {
    pub const RING_ACCOUNT: usize = 0;
    pub const RECORD_ACCOUNT: usize = 1;
    pub const CO_SIGNER_ACCOUNT: usize = 2;
    pub const ACCOUNTS: usize = 3;
    pub const BODY_LEN: usize = 1 + 32;

    /// `sha256(program_id || u32 LE data length || data || addresses)`, `data` from the transfer tag, the length keeps a data byte from reading as an address.
    #[inline(never)]
    pub fn digest<'a>(
        program_id: &Address,
        data: &[u8],
        accounts: impl IntoIterator<Item = &'a Address>,
    ) -> Result<[u8; 32], CustomRingError> {
        let data_len = u32::try_from(data.len())
            .map_err(|_| CustomRingError::InvalidInstructionData)?
            .to_le_bytes();
        let mut preimage = ArrayVec::<&[u8], { 3 + MAX_TRANSFER_ACCOUNTS }>::new();
        preimage.extend([program_id.as_ref(), data_len.as_slice(), data]);
        for address in accounts {
            preimage
                .try_push(address.as_ref())
                .map_err(|_| CustomRingError::TooManyAccounts)?;
        }
        Ok(Sha256::hashv(&preimage)?)
    }

    /// `None` unless `data` is exactly the tag and the body.
    pub fn parse(data: &[u8]) -> Option<Self> {
        let (tag, body) = data.split_first()?;
        (*tag == tag::APPROVE_TRANSFER)
            .then(|| Self::parse_body(body))
            .flatten()
    }

    /// The data after the tag, trailing bytes refused.
    pub fn parse_body(body: &[u8]) -> Option<Self> {
        let (position, digest) = <&[u8; Self::BODY_LEN]>::try_from(body)
            .ok()?
            .split_first()?;
        Some(Self {
            position: *position,
            digest: digest.try_into().ok()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn address(seed: u8) -> Address {
        Address::new_from_array([seed; 32])
    }

    #[test]
    fn the_digest_is_one_sha256_over_the_framed_call() {
        let (program, accounts) = (address(1), [address(2), address(3)]);
        let data = [tag::TRANSFER, 7, 8];
        let preimage = [
            program.as_ref(),
            &3u32.to_le_bytes(),
            &data,
            accounts[0].as_ref(),
            accounts[1].as_ref(),
        ]
        .concat();
        let digest = TransferApproval::digest(&program, &data, &accounts).unwrap();
        // TS `transfer-approval.test.ts` pins the same value.
        assert_eq!(
            hex::encode(digest),
            "2ed0f53961ae51c9a8751a719a070cc01715ad2b4d73ad2c2c59f2be419a9356"
        );
        assert_eq!(digest, Sha256::hash(&preimage).unwrap());
    }

    #[test]
    fn the_digest_binds_the_program_the_data_and_the_account_order() {
        let digest = |program: u8, data: &[u8], accounts: &[Address]| {
            TransferApproval::digest(&address(program), data, accounts).unwrap()
        };
        let accounts = [address(2), address(3)];
        let base = digest(1, &[tag::TRANSFER, 7], &accounts);
        assert_ne!(base, digest(9, &[tag::TRANSFER, 7], &accounts));
        assert_ne!(base, digest(1, &[tag::TRANSFER, 8], &accounts));
        assert_ne!(
            base,
            digest(1, &[tag::TRANSFER, 7], &[address(3), address(2)])
        );
        // 32 data bytes moved into the account list.
        let shifted = [[tag::TRANSFER; 32].as_slice(), address(2).as_ref()].concat();
        assert_ne!(
            digest(1, &shifted, &[address(3)]),
            digest(1, &[tag::TRANSFER; 32], &accounts)
        );
    }

    #[test]
    fn a_list_past_the_transfer_ceiling_is_refused() {
        let accounts = vec![address(2); MAX_TRANSFER_ACCOUNTS + 1];
        let digest = |count| TransferApproval::digest(&address(1), &[], &accounts[..count]);
        assert!(digest(MAX_TRANSFER_ACCOUNTS).is_ok());
        assert_eq!(
            digest(MAX_TRANSFER_ACCOUNTS + 1),
            Err(CustomRingError::TooManyAccounts)
        );
    }
}
