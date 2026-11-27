/** Rust `zolana_ring_keys::ListSaltHolder`. */
import { addressBytes, poseidon } from "@heliuslabs/spr/internal";
import type { Bytes32, ViewingKey } from "@heliuslabs/spr/keypair";
import { expand, extract } from "@noble/hashes/hkdf.js";
import { sha256 } from "@noble/hashes/sha2.js";
import type { Address } from "@solana/kit";

import { domainField, RingDomain } from "../derivation/ring-domain.js";

/** Above the field modulus, so no merge of a real nullifier derives a salt. */
const LIST_SALT_LABEL = (() => {
  const label = new Uint8Array(32);
  label.set(new TextEncoder().encode("CRING/list_salt"));
  return label as Bytes32;
})();

/** The extract of zolana's view root, the label keeps the salt apart from every viewing role. */
const VIEW_ROOT_SALT = new TextEncoder().encode("TSPP/view_root/x25519/v1");
const LIST_SALT_INFO = new TextEncoder().encode("CRING/mrg_seed/v1");

/** `key` is the identity viewing key, no root the auditor holds reaches another ring's salt. */
export function listSaltOf(key: ViewingKey, ringProgramId: Address): Bytes32 {
  const secret = key.secretBytes();
  let viewRoot: Uint8Array | undefined;
  let expanded: Uint8Array | undefined;
  try {
    viewRoot = extract(sha256, secret, VIEW_ROOT_SALT);
    const info = new Uint8Array(LIST_SALT_INFO.length + 64);
    info.set(LIST_SALT_INFO);
    info.set(addressBytes(ringProgramId, "ring program id"), LIST_SALT_INFO.length);
    info.set(LIST_SALT_LABEL, LIST_SALT_INFO.length + 32);
    expanded = expand(sha256, viewRoot, info, 31);
    const salt = new Uint8Array(32);
    salt.set(expanded, 1);
    salt[31] = (salt[31] as number) | 1;
    return salt as Bytes32;
  } finally {
    secret.fill(0);
    viewRoot?.fill(0);
    expanded?.fill(0);
  }
}

/** `Poseidon(CRPT, list_salt)`, final in the tracking leaf. */
export function listTagOf(listSalt: Bytes32): Bytes32 {
  return poseidon([domainField(RingDomain.listTag), listSalt]) as Bytes32;
}
