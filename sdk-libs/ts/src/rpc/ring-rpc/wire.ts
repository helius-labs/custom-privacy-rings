import { getBase58Decoder, getBase58Encoder, getBase64Decoder } from "@solana/kit";
import type { Address, Bytes32, Bytes33 } from "@heliuslabs/spr/interface";
import {
  isCanonicalField,
  wireDecoder,
  equalBytes,
  assetField,
  type indexerTypes,
} from "@heliuslabs/spr/internal";
import { X25519PublicKey, KeypairError } from "@heliuslabs/spr/keypair";
import { RingError } from "../../protocol/error.js";
import { wirePrimitives } from "../../protocol/wire-primitives.js";

export type Hash = indexerTypes.Hash;

export const base58Decoder = getBase58Decoder();
export const base58Encoder = getBase58Encoder();
export const base64Decoder = getBase64Decoder();
export const encoder = new TextEncoder();

/** A BN254 scalar the reader hashes, a non-canonical encoding is malformed. */
export function canonicalHash(input: unknown, path: string): Bytes32 {
  const bytes = hash(input, path);
  if (!isCanonicalField(bytes)) throw invalid(path);
  return bytes;
}

/** The commitment binds the asset field alone, a named mint must derive it. */
export function namedMint(wire: Record<string, unknown>, asset: Bytes32, path: string): Address {
  const mint = address(wire["mint"], `${path}.mint`);
  if (!equalBytes(assetField(mint), asset)) throw invalid(`${path}.mint`);
  return mint;
}

export function u32(value: unknown, path: string): number {
  return count(value, path, 0xffff_ffff);
}

export function u16(value: unknown, path: string): number {
  return count(value, path, 0xffff);
}

export function invalid(path: string): RingError {
  return new RingError("RING_RPC", {
    details: { reason: "invalid response", path },
  });
}

export const { record, list, string, address, boolean, signature, signedInteger, base64, base58 } =
  wireDecoder(invalid);
export const { u64, count, hash } = wirePrimitives((path) => {
  throw invalid(path);
});

export function invalidRequest(field: string): RingError {
  return new RingError("RING_RPC", {
    details: { reason: "invalid request", field },
  });
}

export const { address: requestAddress, record: requestRecord } = wireDecoder(invalidRequest);

export function exactRequestRecord(
  value: unknown,
  allowed: ReadonlySet<string>,
  required: ReadonlySet<string>,
  field: string,
): Record<string, unknown> {
  return exactFields(requestRecord(value, field), allowed, required, invalidRequest, field);
}

/** Rust `deny_unknown_fields` on a response. */
export function exactRecord(
  value: unknown,
  allowed: ReadonlySet<string>,
  required: ReadonlySet<string>,
  path: string,
): Record<string, unknown> {
  return exactFields(record(value, path), allowed, required, invalid, path);
}

function exactFields(
  candidate: Record<string, unknown>,
  allowed: ReadonlySet<string>,
  required: ReadonlySet<string>,
  refuse: (field: string) => RingError,
  field: string,
): Record<string, unknown> {
  const invalidField = Reflect.ownKeys(candidate).find(
    (key) => typeof key !== "string" || !allowed.has(key),
  );
  if (invalidField !== undefined) throw refuse(`${field}.shape`);
  const missing = [...required].find((key) => !Object.hasOwn(candidate, key));
  if (missing !== undefined) throw refuse(`${field}.${missing}`);
  return candidate;
}

export function requestBytes(value: unknown, length: number, field: string): Uint8Array {
  if (!(value instanceof Uint8Array) || value.length !== length) throw invalidRequest(field);
  return new Uint8Array(value);
}

/** Bounded to the safe integer range, `Number` in the wire encoding stays exact. */
export function checkedUnixTimestamp(value: unknown, field: string): bigint {
  if (typeof value !== "bigint" || value < 0n || value > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw invalidRequest(field);
  }
  return value;
}

export function x25519Key(value: unknown, path: string): X25519PublicKey {
  const bytes = base64(value, path);
  if (bytes.length !== 33) throw invalid(path);
  try {
    return X25519PublicKey.fromBytes(bytes as Bytes33);
  } catch (cause) {
    if (!(cause instanceof KeypairError)) throw cause;
    throw invalid(path);
  }
}
