import type { MessagePartialSigner } from "@solana/kit";
import type { Address, Bytes32 } from "@heliuslabs/spr/interface";
import { addressBytes, copyBytes } from "@heliuslabs/spr/internal";
import { signMessage, type RingReadSigner } from "../../protocol/ring-rpc/read-signer.js";
import { checkedReaderKey, readerKeyBytes } from "../../protocol/membership/reader-access.js";
import { base58Decoder, checkedUnixTimestamp, base64Decoder } from "./wire.js";

/** Rust `ReadTarget`, one ring behind one ring rpc on one cluster, every signed read names all three. */
export interface RingReadTarget {
  /** The genesis hash. */
  readonly cluster: Bytes32;
  /** The service key the caller pinned, never one the service reports. */
  readonly service: Address;
  readonly ringProgramId: Address;
}

/** Rust `ServiceTarget`, one ring rpc on one cluster, a relayed deposit or merge names it beside the ring. */
export type RingServiceTarget = Omit<RingReadTarget, "ringProgramId">;

/** Rust `ReadTarget::lines`. */
export function targetLines(target: RingReadTarget): readonly string[] {
  return [
    `cluster: ${base58Decoder.decode(target.cluster)}`,
    `service: ${target.service}`,
    `ring: ${target.ringProgramId}`,
  ];
}

/** Rust `wallet_line`, no line for a ring-wide request. */
export function walletLines(wallet: Address | undefined): readonly string[] {
  return wallet === undefined ? [] : [`wallet: ${wallet}`];
}

export function checkedReadTarget(target: RingReadTarget): RingReadTarget {
  addressBytes(target.service, "service");
  addressBytes(target.ringProgramId, "ringProgramId");
  return Object.freeze({
    cluster: copyBytes(target.cluster, 32, "cluster") as Bytes32,
    service: target.service,
    ringProgramId: target.ringProgramId,
  });
}

export function messageSignerReader(signer: MessagePartialSigner): RingReadSigner {
  return Object.freeze({
    reader: readerKeyBytes(checkedReaderKey(signer.address)),
    sign: (message: Uint8Array) => signMessage(signer, message),
  });
}

/** Minted once per request. */
export function freshNonce(): Bytes32 {
  const nonce = new Uint8Array(32);
  globalThis.crypto.getRandomValues(nonce);
  return nonce as Bytes32;
}

export function timestampOrNow(timestamp: bigint | undefined): bigint {
  return timestamp === undefined
    ? BigInt(Math.floor(Date.now() / 1000))
    : checkedUnixTimestamp(timestamp, "timestamp");
}

export function authWire(
  auth: Readonly<{ timestamp: bigint; nonce: Bytes32; signature: Uint8Array }>,
): Readonly<{ timestamp: number; nonce: string; signature: string }> {
  return {
    timestamp: Number(auth.timestamp),
    nonce: base64Decoder.decode(auth.nonce),
    signature: base64Decoder.decode(auth.signature),
  };
}
