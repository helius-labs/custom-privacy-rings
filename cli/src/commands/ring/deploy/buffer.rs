use anyhow::{ensure, Context as _, Result};
use sha2::{Digest, Sha256};
use solana_account::Account;
use solana_address::{Address, MAX_SEED_LEN};
use solana_loader_v3_interface::{instruction as loader, state::UpgradeableLoaderState};
use solana_message::legacy::Message;
use solana_packet::PACKET_DATA_SIZE;
use solana_system_interface::instruction as system;
use spr_client::Rpc;

use super::{system_wallet, ProgramUpload, UploadBuffer, LOADER};
use crate::Context;

const MIN_DEPLOY_FEE_BUDGET: u64 = 20_000_000;

impl<'a> ProgramUpload<'a> {
    pub(super) fn bind(self, program: Address) -> Result<UploadBuffer<'a>> {
        let digest = Sha256::new()
            .chain_update(b"zolana-ring-loader-buffer-v1")
            .chain_update(program.as_ref())
            .chain_update(self.binary.sha256)
            .finalize();
        let seed = hex::encode(&digest[..MAX_SEED_LEN / 2]);
        let address = Address::create_with_seed(&self.authority.address(), &seed, &LOADER)?;
        Ok(UploadBuffer {
            address,
            seed,
            source: self,
        })
    }
}

impl UploadBuffer<'_> {
    pub(super) fn fund_and_write(&self, ctx: &Context, required_rent: u64) -> Result<()> {
        let account = ctx.rpc().get_account(self.address)?;
        let contents = account
            .as_ref()
            .map(|account| self.contents(account))
            .transpose()?
            .flatten();
        let size = UpgradeableLoaderState::size_of_buffer(self.source.bytes.len());
        let rent = ctx.rpc().get_minimum_balance_for_rent_exemption(size)?;
        let held = account
            .as_ref()
            .map_or(0, |account| account.lamports.min(rent));
        let payer = self.source.authority.address();
        let write = |offset, bytes| loader::write(&self.address, &payer, offset, bytes);
        let empty = write(0, Vec::new());
        // The loader limits decoded instruction data to one packet.
        let write_bytes = PACKET_DATA_SIZE - empty.data.len();
        let mut pending = Vec::new();
        for (index, chunk) in self.source.bytes.chunks(write_bytes).enumerate() {
            let offset = index * write_bytes;
            let stored = contents.map(|bytes| &bytes[offset..offset + chunk.len()]);
            if let Some(stored) = stored {
                if stored == chunk {
                    continue;
                }
                ensure!(
                    stored
                        .iter()
                        .zip(chunk)
                        .all(|(actual, wanted)| *actual == 0 || actual == wanted),
                    "upload buffer {} differs at byte {offset}",
                    self.address
                );
            }
            if chunk.iter().any(|byte| *byte != 0) {
                pending.push((offset, chunk));
            }
        }

        let (blockhash, _) = ctx.rpc().get_latest_blockhash()?;
        let quote = Message::new_with_blockhash(&[empty], Some(&payer), &blockhash);
        let fee_per_signature = ctx.rpc().client().get_fee_for_message(&quote)?;
        // Buffer creation and initial deployment require three signatures in total.
        let signatures = u64::try_from(pending.len())? + 3;
        let required = fee_per_signature
            .checked_mul(signatures)
            .and_then(|fee| {
                required_rent
                    .saturating_sub(held)
                    .checked_add(fee.max(MIN_DEPLOY_FEE_BUDGET))
            })
            .context("deploy funding exceeds the lamport limit")?;
        ctx.fund(payer, required)?;
        if contents.is_none() {
            let mut instructions = loader::create_buffer(
                &payer,
                &self.address,
                &payer,
                rent,
                self.source.bytes.len(),
            )?;
            instructions.splice(
                ..1,
                [
                    system::transfer(&payer, &self.address, rent - held),
                    system::allocate_with_seed(
                        &self.address,
                        &payer,
                        &self.seed,
                        u64::try_from(size)?,
                        &LOADER,
                    ),
                ],
            );
            ctx.send_operator(&[self.source.authority], &instructions)?;
        }
        for (offset, chunk) in pending {
            let write = write(u32::try_from(offset)?, chunk.to_vec());
            ctx.send_operator(&[self.source.authority], &[write])
                .with_context(|| {
                    format!("write to buffer {} failed at byte {offset}", self.address)
                })?;
        }
        let uploaded = ctx
            .rpc()
            .get_account(self.address)?
            .context("upload buffer is missing")?;
        ensure!(
            self.contents(&uploaded)? == Some(self.source.bytes),
            "upload buffer differs from the program binary"
        );
        Ok(())
    }

    pub(super) fn refund(&self, ctx: &Context) -> Result<()> {
        if let Some(account) = ctx.rpc().get_account(self.address)? {
            if self.contents(&account)?.is_some() {
                let payer = self.source.authority.address();
                let close = loader::close(&self.address, &payer, &payer, false);
                ctx.send_operator(&[self.source.authority], &[close])?;
                println!("refunded {}", self.address);
                return Ok(());
            }
        }
        println!("no loader buffer at {}", self.address);
        Ok(())
    }

    fn contents<'a>(&self, account: &'a Account) -> Result<Option<&'a [u8]>> {
        if system_wallet(account) {
            return Ok(None);
        }
        ensure!(
            account.owner == LOADER && !account.executable,
            "invalid upload buffer owner or executable flag"
        );
        let (header, bytes) = account
            .data
            .split_at_checked(UpgradeableLoaderState::size_of_buffer_metadata())
            .context("truncated upload buffer")?;
        let expected = UpgradeableLoaderState::Buffer {
            authority_address: Some(self.source.authority.address()),
        };
        ensure!(
            wincode::deserialize::<UpgradeableLoaderState>(header)? == expected,
            "invalid upload buffer state or authority"
        );
        ensure!(
            bytes.len() == self.source.bytes.len(),
            "invalid upload buffer capacity"
        );
        Ok(Some(bytes))
    }
}
