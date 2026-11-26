#!/usr/bin/env bash
# Ring localnet, one SPL test mint and a derived mode Ring RPC for the demo, then demo.env by tools/demo-env.md.
# DEMO_LOCALNET_DRY_RUN=1 starts nothing and writes demo.env from fixed fake values.
# write_env reads each key by name.
# shellcheck disable=SC2034
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

workdir=target/ring-localnet
env_file="$workdir/demo.env"
just_bin="${JUST:-just}"
keys=(
    RING_SOLANA_RPC_URL RING_INDEXER_URL RING_PROVER_URL RING_RPC_URL
    RING_GENESIS_HASH RING_RPC_SERVICE_KEY RING_RPC_COSIGNER RING_ENROLLMENT_WRITER
    RING_STORAGE_PROGRAM_ID RING_LOCALNET_AUTHORITY_SEED RING_TEST_MINT
)
unset "${keys[@]}"
# Public seed, the localnet account snapshots name its address as the protocol authority.
RING_LOCALNET_AUTHORITY_SEED=7a6f6c616e61206c6f63616c6e657420736e617073686f7420617574686f7269
localnet_authority=HF3DijLymE9gVYeG3kX1N19WZXu5BH5yWBgsvnVtm7tw
relay_budget_lamports=50000000000
relay_signer_budget_lamports=2000000000
enrollment_ring_budget_lamports=100000000
enrollment_max_rings=256

write_env() {
    local key value
    mkdir -p "$workdir"
    for key in "${keys[@]}"; do
        value="${!key:-}"
        if [[ -z "$value" || "$value" == *[[:space:]]* ]]; then
            echo "demo.env has no usable $key, found '$value'" >&2
            exit 1
        fi
        printf '%s=%s\n' "$key" "$value"
    done > "$env_file.partial"
    mv "$env_file.partial" "$env_file"
    echo "wrote $env_file"
}

if [[ "${DEMO_LOCALNET_DRY_RUN:-}" == 1 ]]; then
    RING_SOLANA_RPC_URL=http://127.0.0.1:8899
    RING_INDEXER_URL=http://127.0.0.1:8786
    RING_PROVER_URL=http://127.0.0.1:3001
    RING_RPC_URL=http://127.0.0.1:8785
    RING_GENESIS_HASH=2UGYmFi1AVQsAJqJEu2uYknLkgU79jRwYs83JLx9css3
    RING_RPC_SERVICE_KEY=F8Tm9XxR4WvJAhYZgggmmqAkfJPtp8TGrwKRRLoKrXaa
    RING_RPC_COSIGNER=FJPW8GJpc1Tv2NkuFB7S2Fi46QM9zjmskVDiNozJRGfc
    RING_ENROLLMENT_WRITER=CGPp9KLbTy9K63rTQLHvrtMqqJmiXgYMRxuWFD1WJ6P6
    RING_STORAGE_PROGRAM_ID=rnGSTosTQ9gToNQP44THEZph8xtBtuGa5FEwzhKrWjL
    RING_TEST_MINT=AMcKFPiWHNHMhFx6NRJge9JRLY2yRAv6xdxsJHRWboiu
    write_env
    exit 0
fi

setting() {
    "$just_bin" --evaluate "$1"
}

"$just_bin" build-ring-rpc ring-localnet
RING_SOLANA_RPC_URL="$(setting localnet-rpc-url)"
RING_INDEXER_URL="$(setting localnet-ring-indexer-url)"
RING_PROVER_URL="$(setting localnet-prover-url)"
ring_rpc_port="$(setting localnet-ring-rpc-port)"
RING_RPC_URL="http://127.0.0.1:$ring_rpc_port"
genesis_programs="$(setting ring-genesis-programs)"
read -r flag RING_STORAGE_PROGRAM_ID _ <<<"$genesis_programs"
if [[ "$flag" != --sbf-program ]]; then
    echo "ring-genesis-programs names no storage program, found '$genesis_programs'" >&2
    exit 1
fi
RING_GENESIS_HASH="$(curl -sf --max-time 10 -H 'Content-Type: application/json' \
    -d '{"jsonrpc":"2.0","id":1,"method":"getGenesisHash"}' "$RING_SOLANA_RPC_URL" | jq -r '.result // empty')"

zolana=zolana/target/debug/zolana
mint_wallet="$workdir/test-mint-authority.json"
export ZOLANA_CONFIG_DIR="$PWD/$workdir/zolana-config"
mkdir -p "$ZOLANA_CONFIG_DIR"
"$zolana" config set --rpc-url "$RING_SOLANA_RPC_URL" \
    --indexer-url "$RING_INDEXER_URL" --prover-url "$RING_PROVER_URL" >/dev/null
"$zolana" wallet new --outfile "$mint_wallet" >/dev/null
mint_output="$("$zolana" dev pool test-mint --keypair "$mint_wallet" --authority-path "$mint_wallet" \
    --airdrop-lamports 20000000000 --amount 1000000000000000)"
RING_TEST_MINT="$(sed -n 's/^ok test_mint mint=\([^ ]*\).*/\1/p' <<<"$mint_output")"
if [[ -z "$RING_TEST_MINT" ]]; then
    echo "test-mint printed no mint" >&2
    echo "$mint_output" >&2
    exit 1
fi
# The demo signs ring activation with the localnet authority.
solana airdrop 100 "$localnet_authority" --url "$RING_SOLANA_RPC_URL" >/dev/null

ring_dir="$workdir/ring-rpc"
# Ring RPC reads finalized ring history on Surfpool only under the fixture.
export ZOLANA_RING_SURFPOOL_FIXTURE=1
# Ring RPC signs through the local KMS, every key lives only while it runs.
RING_RPC_LOCALNET_KMS="$(tools/ring-rpc-local.sh kms "$(setting localnet-kms-port)" "$workdir/kms.log")"
export RING_RPC_LOCALNET_KMS
tools/ring-rpc-local.sh custody "$ring_dir" "$RING_SOLANA_RPC_URL" co-signer enrollment-writer
# The writer serves a ring only after its policy authority calls registerEnrollmentRing.
tools/ring-rpc-local.sh serve "$ring_dir" "$ring_rpc_port" "$workdir/ring-rpc.log" \
    --indexer-url "$RING_INDEXER_URL" --rpc-url "$RING_SOLANA_RPC_URL" \
    --prover-url "$RING_PROVER_URL" --store-url "sqlite://$PWD/$workdir/ring-rpc.sqlite?mode=rwc" \
    --relay-budget-lamports "$relay_budget_lamports" \
    --relay-signer-budget-lamports "$relay_signer_budget_lamports" \
    --allow-origin "${DEMO_ALLOW_ORIGINS:-http://localhost:3000,http://localhost:3043,http://localhost:3044}" \
    --enrollment-ring-budget-lamports "$enrollment_ring_budget_lamports" \
    --enrollment-max-rings "$enrollment_max_rings"
RING_RPC_SERVICE_KEY="$(tools/ring-rpc-local.sh address "$ring_dir" service)"
RING_RPC_COSIGNER="$(tools/ring-rpc-local.sh address "$ring_dir" co_signer)"
RING_ENROLLMENT_WRITER="$(tools/ring-rpc-local.sh address "$ring_dir" enrollment_applier)"
write_env
echo "  test mint $RING_TEST_MINT, authority wallet $mint_wallet"
echo "  ring rpc  $RING_RPC_URL, log $workdir/ring-rpc.log"
echo "  kms       $RING_RPC_LOCALNET_KMS, log $workdir/kms.log"
echo "stop with 'just ring-localnet-stop'"
