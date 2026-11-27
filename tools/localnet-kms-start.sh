# shellcheck shell=bash
# Sourced, never executed.
localnet_kms_pid=""

start_localnet_kms() {
    local ready_file="$1"
    (ulimit -c 0 && exec node "${BASH_SOURCE[0]%/*}/localnet-kms.mjs" 0) >"$ready_file" &
    localnet_kms_pid=$!
    local waited=0
    until [[ -s "$ready_file" ]]; do
        kill -0 "$localnet_kms_pid" 2>/dev/null || { echo "local KMS exited before listening" >&2; return 1; }
        ((++waited <= 600)) || { echo "local KMS did not report a listener" >&2; return 1; }
        sleep 0.1
    done
    IFS= read -r RING_RPC_LOCALNET_KMS <"$ready_file" || { echo "local KMS did not report a listener" >&2; return 1; }
    export RING_RPC_LOCALNET_KMS
}

stop_localnet_kms() {
    [[ -n "$localnet_kms_pid" ]] || return 0
    kill "$localnet_kms_pid" 2>/dev/null || true
    wait "$localnet_kms_pid" 2>/dev/null || true
    localnet_kms_pid=""
}
