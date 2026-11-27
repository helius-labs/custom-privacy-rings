//! `address`, the `zolana:` payment request an anonymous ring pays a member through.

use anyhow::{ensure, Result};
use custom_ring_interface::RingMode;
use spr_keypair::{viewing_key::ViewTag, ShieldedKeypair};
use zolana_ring_keys::{RingAddress, RingPaymentRequest};

use crate::{file, AddressArgs, Context};

pub fn run(ctx: &Context, args: AddressArgs) -> Result<()> {
    let config = ctx.ring_config()?;
    ensure!(
        config.mode == RingMode::Anonymous,
        "a `zolana:` request pays an anonymous ring, ring {} is not one",
        ctx.ring.program_id()
    );
    let keypair = match &args.keypair {
        Some(path) => file::read_keypair(&ctx.path(path))?,
        None => ctx.sender()?,
    };
    let member = ShieldedKeypair::from_keypair(&keypair)?;
    let held = ctx.ring_keys(&member)?;
    let address = RingAddress::enrolled(&held, ctx.ring.program_id())?;
    // The store takes the request's index before the tag is printed, a sync lists it from then on.
    let tag = if args.bootstrap && args.request_count.is_none() {
        None
    } else {
        ctx.anonymous_wallet_store(held.identity_viewing_pubkey())
            .update(|wallet| {
                let mut history = wallet
                    .history()
                    .clone()
                    .with_request_count(args.request_count.unwrap_or_default());
                let mut tag = None;
                if !args.bootstrap {
                    let minted = history.request_tag(&held)?;
                    tag = Some(minted.tag);
                    history = minted.history;
                }
                wallet.update_history(wallet.history().advance_to(history))?;
                Ok(tag)
            })?
    };
    println!("{}", args.request(address, tag));
    Ok(())
}

impl AddressArgs {
    fn request(self, address: RingAddress, tag: Option<ViewTag>) -> RingPaymentRequest {
        RingPaymentRequest {
            amount: self.amount,
            mint: self.mint.map(|mint| mint.0),
            label: self.label,
            tag,
            ..RingPaymentRequest::new(address)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use bytemuck::Zeroable;
    use clap::Parser;
    use custom_ring_interface::{NewRing, RingPolicy};
    use serde::Deserialize;
    use serde_json::json;
    use solana_address::Address;
    use solana_rpc_client::rpc_client::RpcClient;
    use solana_rpc_client_api::request::RpcRequest;
    use spr_client::{AsyncProverClient, SolanaRpc, SprIndexer};
    use spr_indexer_api::Base64String;
    use spr_keypair::{ShieldedAddress, ViewingKey};

    use super::*;
    use crate::{
        context::tests::open_with_urls, wallet::ANONYMOUS_RING_DIR, Cli, Command, ZolanaServices,
    };

    const VECTORS: &str = include_str!("../../../../../test-vectors/wallet/ring_address.json");

    #[derive(Deserialize)]
    struct Vectors {
        valid: Vec<Case>,
    }

    #[derive(Deserialize)]
    struct Case {
        name: String,
        uri: String,
        recipient: String,
        ring_program_id: String,
        list_salt: String,
        amount: Option<String>,
        mint: Option<String>,
        label: Option<String>,
        tag: Option<String>,
    }

    fn parse(flags: &[&str]) -> AddressArgs {
        let argv = ["zolana-ring", "address"].iter().chain(flags).copied();
        match Cli::try_parse_from(argv).unwrap().command {
            Command::Address(args) => args,
            _ => panic!("address"),
        }
    }

    #[test]
    fn the_printed_request_matches_the_keypair_vectors() {
        for case in serde_json::from_str::<Vectors>(VECTORS).unwrap().valid {
            let recipient = hex::decode(&case.recipient).unwrap().try_into().unwrap();
            let mut flags = Vec::new();
            for (flag, value) in [
                ("--amount", &case.amount),
                ("--mint", &case.mint),
                ("--label", &case.label),
            ] {
                if let Some(value) = value {
                    flags.extend([flag, value.as_str()]);
                }
            }
            let address = RingAddress {
                recipient: ShieldedAddress::from_bytes(&recipient).unwrap(),
                ring_program_id: case.ring_program_id.parse().unwrap(),
                list_salt: hex::decode(&case.list_salt).unwrap().try_into().unwrap(),
            };
            let tag = case
                .tag
                .as_ref()
                .map(|tag| hex::decode(tag).unwrap().try_into().unwrap());
            assert_eq!(
                parse(&flags).request(address, tag).to_string(),
                case.uri,
                "{}",
                case.name
            );
        }
    }

    /// Under its own root, a mocked RPC answers `reads` reads of an anonymous ring's policy account.
    fn anonymous_ring(test: &str, reads: usize) -> Context {
        let (rpc, indexer, prover) = (
            "http://127.0.0.1:8899",
            "http://127.0.0.1:8784",
            "http://127.0.0.1:3001",
        );
        let mut ctx = open_with_urls(rpc, indexer, prover).unwrap();
        ctx.root = std::env::temp_dir().join(format!("ring-address-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ctx.root);
        let ring = ctx.ring.program_id();
        let (_, bump) = Address::find_program_address(&[RingPolicy::SEED], &ring);
        let policy = NewRing {
            bump,
            mode: RingMode::Anonymous,
            roles: Zeroable::zeroed(),
            auditor_pubkey: *ViewingKey::new().pubkey().as_bytes(),
            created_slot: 0,
            records: Address::default(),
            tracking: Address::default(),
        }
        .account();
        let account = json!({
            "lamports": 1,
            "data": [Base64String(bytemuck::bytes_of(&*policy).to_vec()), "base64"],
            "owner": ring.to_string(),
            "executable": false,
            "rentEpoch": 0,
        });
        let answer = json!({ "context": { "slot": 1 }, "value": account });
        let answers = (0..reads).map(|_| (RpcRequest::GetAccountInfo, answer.clone()));
        ctx.zolana = ZolanaServices {
            rpc: SolanaRpc::with_client(RpcClient::new_mock_with_mocks_map(
                "succeeds",
                answers.collect(),
            )),
            indexer: SprIndexer::new(indexer),
            prover: AsyncProverClient::local(),
            ring_prover: AsyncProverClient::local(),
        }
        .connect();
        ctx
    }

    fn stored_request_count(ctx: &Context) -> u64 {
        let member = ShieldedKeypair::from_keypair(&ctx.sender().unwrap()).unwrap();
        let store = ctx.anonymous_wallet_store(member.viewing_pubkey());
        store.load().unwrap().history().current().request_count()
    }

    #[test]
    fn a_mint_and_a_stated_count_are_stored_and_the_bootstrap_address_stores_nothing() {
        let ctx = anonymous_ring("address-mint", 2);
        run(&ctx, parse(&["--bootstrap"])).unwrap();
        let store_dir = ctx.path(Path::new(ANONYMOUS_RING_DIR));
        assert!(!store_dir.exists(), "the bootstrap address");
        run(&ctx, parse(&[])).unwrap();
        assert_eq!(stored_request_count(&ctx), 1);

        let restored = anonymous_ring("address-restored", 1);
        run(&restored, parse(&["--request-count", "200"])).unwrap();
        assert_eq!(stored_request_count(&restored), 201, "index 200");
        for root in [ctx.root, restored.root] {
            std::fs::remove_dir_all(root).unwrap();
        }
    }
}
