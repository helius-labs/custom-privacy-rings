/** The vouch every ring transfer proof verifies, Go `policy/voucher.go`. */
import { ed25519 } from "@noble/curves/ed25519.js";
import { copyBytes } from "@heliuslabs/spr/internal";
import type { Bytes32, Bytes64, RequestContext } from "@heliuslabs/spr/interface";
import { SigningKey, type ShieldedPublicKey } from "@heliuslabs/spr/keypair";

import { RingDomain, domainHash } from "../crypto/derivation/ring-domain.js";
import { RingError } from "../protocol/error.js";
import type { VoucherProofInput } from "../rpc/prover/ring.js";

/** A wallet on the ring's allow list, Rust `VouchSigner`. */
export interface VouchSigner {
  publicKey(): ShieldedPublicKey;
  /** RFC 8032 Ed25519 over the 32 bytes of `vouchDigest` with no message header. */
  sign(digest: Uint8Array, context?: RequestContext): Promise<Bytes64>;
}

/** The transfer a voucher signs for, Rust `VouchMessage`. */
export interface VouchMessage {
  readonly privateTxHash: Bytes32;
  readonly withdrawalHash: Bytes32;
}

/** `Poseidon(CRVC, private_tx_hash, withdrawal_hash)`, the voucher signs its 32 big endian bytes. */
export function vouchDigest(message: VouchMessage): Bytes32 {
  return domainHash(RingDomain.vouch, [
    copyBytes(message.privateTxHash),
    copyBytes(message.withdrawalHash),
  ]);
}

/** A fresh throwaway key signs a transfer nobody vouches for, the proof verifies one signature on every frame. */
export async function signVouch(
  voucher: VouchSigner | undefined,
  message: VouchMessage,
  context?: RequestContext,
): Promise<VoucherProofInput> {
  const digest = vouchDigest(message);
  if (voucher !== undefined) {
    const publicKey = voucher.publicKey().ed25519();
    const signature = await voucher.sign(digest, context);
    if (signature.length !== 64 || !ed25519.verify(signature, digest, publicKey, { zip215: false }))
      throw new RingError("RING_VOUCHER_SIGNATURE_MISMATCH");
    return Object.freeze({ publicKey, signature });
  }
  const throwaway = SigningKey.generate();
  try {
    return Object.freeze({
      publicKey: throwaway.publicKey().ed25519(),
      signature: throwaway.sign(digest),
    });
  } finally {
    throwaway.destroy();
  }
}
