import { createSignableMessage, type MessagePartialSigner } from "@solana/kit";
import type { Bytes64 } from "@heliuslabs/spr/interface";
import { copyBytes } from "@heliuslabs/spr/internal";
import { RingError } from "../error.js";

/** `reader` is the 34-byte scheme-tagged key of `readerKeyBytes`, a P-256 reader signs through WebAuthn only. */
export interface RingReadSigner {
  readonly reader: Uint8Array;
  sign(message: Uint8Array): Promise<Uint8Array | WebAuthnSignature>;
}

/** Mirrors Rust `WebAuthnAssertion` plus the DER signature. */
export interface WebAuthnSignature {
  readonly signature: Uint8Array;
  readonly authenticatorData: Uint8Array;
  readonly clientDataJSON: Uint8Array;
}

/** One Ed25519 signature from a kit message signer over the raw bytes. */
export async function signMessage(
  signer: MessagePartialSigner,
  message: Uint8Array,
): Promise<Bytes64> {
  const [signatures] = await signer.signMessages([createSignableMessage(message)]);
  const signature = signatures?.[signer.address];
  if (signature === undefined) {
    throw new RingError("RING_RPC", {
      details: { reason: "signer returned no signature" },
    });
  }
  return copyBytes(signature, 64, "signature") as Bytes64;
}
