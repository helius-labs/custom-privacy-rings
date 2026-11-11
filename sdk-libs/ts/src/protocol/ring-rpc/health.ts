import type { Address } from "@heliuslabs/spr/interface";
import { wireDecoder } from "@heliuslabs/spr/internal";

import { RingError } from "../error.js";

export type RingKeyMode = "local" | "derived";

/** Rust `ReaderGrantMode`, where the service reads reader grants. */
export type RingReaderGrantMode = "tree" | "database";

/** Mirrors Rust `HealthResponse`, a key is absent when the service runs without its role. */
export interface RingRpcHealth {
  readonly mode: RingKeyMode;
  readonly servicePublicKey: Address;
  readonly coSignerPublicKey?: Address;
  readonly storageAuthorityPublicKey?: Address;
  /** Pays every relayed transaction and managed merge. */
  readonly feePayerPublicKey?: Address;
  /** Signs each `EnrollBatch` under a ring's `enrollmentWriter` role. */
  readonly enrollmentWriterPublicKey?: Address;
  readonly relay: boolean;
  readonly recordWriter: boolean;
  /** The enrollment writer runs with a nonzero budget. */
  readonly enrollmentWrites: boolean;
  readonly readerGrants: RingReaderGrantMode;
}

function invalid(path: string): RingError {
  return new RingError("RING_RPC", {
    details: { reason: "invalid response", path },
  });
}

const { record, address, boolean } = wireDecoder(invalid);

/** Unknown fields pass, so a newer service still answers an older client. */
export function decodeRingRpcHealth(value: unknown): RingRpcHealth {
  const wire = record(value, "result");
  const mode = wire["mode"];
  if (mode !== "local" && mode !== "derived") throw invalid("result.mode");
  const readerGrants = wire["readerGrants"];
  if (readerGrants !== "tree" && readerGrants !== "database") {
    throw invalid("result.readerGrants");
  }
  const optionalKey = (field: string): Address | undefined => {
    const key = wire[field];
    return key === undefined || key === null ? undefined : address(key, `result.${field}`);
  };
  const coSigner = optionalKey("coSignerPubkey");
  const storageAuthority = optionalKey("storageAuthorityPubkey");
  const feePayer = optionalKey("feePayerPubkey");
  const enrollmentWriter = optionalKey("enrollmentWriterPubkey");
  return Object.freeze({
    mode,
    servicePublicKey: address(wire["servicePubkey"], "result.servicePubkey"),
    ...(coSigner === undefined ? {} : { coSignerPublicKey: coSigner }),
    ...(storageAuthority === undefined ? {} : { storageAuthorityPublicKey: storageAuthority }),
    ...(feePayer === undefined ? {} : { feePayerPublicKey: feePayer }),
    ...(enrollmentWriter === undefined ? {} : { enrollmentWriterPublicKey: enrollmentWriter }),
    relay: boolean(wire["relay"], "result.relay"),
    recordWriter: boolean(wire["recordWriter"], "result.recordWriter"),
    enrollmentWrites: boolean(wire["enrollmentWrites"], "result.enrollmentWrites"),
    readerGrants,
  });
}
