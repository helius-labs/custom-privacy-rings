#!/usr/bin/env bash
start_localnet_kms() {
    local ready_file="$1" attempt
    ulimit -c 0
    node "${BASH_SOURCE[0]%/*}/localnet-kms.mjs" 0 >"$ready_file" &
    kms_pid=$!
    for ((attempt = 0; attempt < 100; attempt++)); do
        [[ -s "$ready_file" ]] && break
        kill -0 "$kms_pid" || return 1
        sleep 0.1
    done
    IFS= read -r RING_LOCALNET_KMS <"$ready_file" || { echo "local KMS did not report a listener" >&2; return 1; }
    export RING_LOCALNET_KMS
}

[[ "${BASH_SOURCE[0]}" == "$0" ]] || return 0
set -euo pipefail

: "${RING_LOCALNET_KMS:?set the local KMS endpoint}"
[[ "$RING_LOCALNET_KMS" =~ ^127\.0\.0\.1:[0-9]+$ ]] || { echo 'expected an IPv4 loopback KMS endpoint' >&2; exit 1; }
ring_rpc="${RING_RPC_BIN:-target/debug/ring-rpc}"
deployment="${RING_RPC_DEPLOYMENT:-localnet}"

create_key() {
    curl --noproxy '*' --fail --silent --show-error -X POST "http://$RING_LOCALNET_KMS/" \
        -H 'Content-Type: application/x-amz-json-1.1' -H 'X-Amz-Target: TrentService.CreateKey' \
        --data "$1" | jq -er .KeyMetadata.Arn
}

local_signer() {
    local arn
    arn="$(create_key '{"KeySpec":"ECC_NIST_EDWARDS25519","KeyUsage":"SIGN_VERIFY"}')"
    "$ring_rpc" signer-ref --localnet-kms "$RING_LOCALNET_KMS" --key-arn "$arn"
}

case "${1:-}" in
    signer)
        local_signer
        ;;
    wrapping-key)
        create_key '{}'
        ;;
    operator)
        program="${2:?expected program signer reference}"
        upgrade="${3:?expected upgrade signer reference}"
        writer="${4:?expected enrollment writer signer reference}"
        admin="$(local_signer)"
        policy="$(local_signer)"
        pauser="$(local_signer)"
        cosign_admin="$(local_signer)"
        jq -n --arg region us-east-1 --arg deployment "$deployment" --arg localnet_kms "$RING_LOCALNET_KMS" \
            --argjson program "$program" --argjson upgrade_authority "$upgrade" --argjson admin "$admin" \
            --argjson policy "$policy" --argjson pauser "$pauser" --argjson cosign_admin "$cosign_admin" \
            --argjson enrollment_writer "$writer" --argjson auditor "${5:-null}" \
            '{ $region, $deployment, $localnet_kms, $program, $upgrade_authority, $admin, $policy, $pauser, $cosign_admin, $enrollment_writer, $auditor, rpc_custody: "rpc-custody.json" }'
        ;;
    rpc)
        genesis="${2:?expected genesis hash}"
        scope="${3:?expected ring program address or derived}"
        writer="${4:?expected enrollment writer signer reference or null}"
        service="$(local_signer)"
        wrap_arn="$(create_key '{}')"
        provision=("$ring_rpc" provision --localnet-kms "$RING_LOCALNET_KMS" --deployment "$deployment" \
            --wrapping-key-arn "$wrap_arn" --genesis-hash "$genesis")
        if [[ "$scope" == derived ]]; then
            auditor="$("${provision[@]}" --purpose auditor-root)"
        else
            auditor="$("${provision[@]}" --purpose auditor --ring-program-id "$scope")"
        fi
        profile="${5:-read}"
        storage=null
        fee_payer=null
        grant_rows=null
        case "$profile" in
            read) ;;
            records|relay) storage="$(local_signer)" ;;
            *) echo "unknown RPC profile" >&2; exit 2 ;;
        esac
        if [[ "$profile" == relay ]]; then
            fee_payer="$(local_signer)"
            grant_rows="$("${provision[@]}" --purpose grant-rows)"
        fi
        jq -n --argjson auditor "$auditor" --argjson service "$service" --argjson enrollment_applier "$writer" \
            --argjson storage_authority "$storage" --argjson fee_payer "$fee_payer" --argjson grant_rows "$grant_rows" \
            '{ $auditor, $service, $enrollment_applier, $storage_authority, $fee_payer, $grant_rows }'
        ;;
    *) echo "usage: localnet-custody.sh signer | wrapping-key | operator PROGRAM UPGRADE WRITER [AUDITOR] | rpc GENESIS RING_OR_DERIVED WRITER_OR_NULL [records|relay]" >&2; exit 2 ;;
esac
