/** Which ring features a prover serves, read from its proving key catalog. */
import { ClientError, type RequestContext } from "@heliuslabs/spr";
import type { ProvingKeyReport } from "@heliuslabs/spr/client";
import { MERGE_SUPPORTED_INPUT_COUNTS, PROVING_KEY_SHA256S } from "@heliuslabs/spr/internal";

import { RingError } from "../protocol/error.js";
import {
  RING_KEYS,
  RING_PROVING_KEY_SHA256S,
  type RingKeyCircuit,
} from "../protocol/proof/proving-keys.js";
import { defined } from "../protocol/defined.js";

export const RING_FEATURES = ["transfer", "deposit", "merge", "enroll", "applyRecords"] as const;
export type RingFeature = (typeof RING_FEATURES)[number];

/** `unverified` when the catalog check failed ahead of the keys. */
export type RingCapabilityReason = "unpinned" | "missing" | "mismatched" | "unverified";

export type RingCapability =
  | Readonly<{ available: true; keyNames: readonly string[] }>
  | Readonly<{
      available: false;
      reason: RingCapabilityReason;
      /** The keys the reason names. */
      keyNames: readonly string[];
      /** The error code of an `unverified` check. */
      code?: string;
    }>;

export type RingCapabilities = Readonly<Record<RingFeature, RingCapability>>;

export type RingCapabilityClient = Readonly<{
  checkRingProvingKeys(context?: RequestContext): Promise<ProvingKeyReport>;
}>;

/** A prover error or an unreachable prover marks each feature unavailable, a caller abort throws. */
export async function ringCapabilities(
  client: RingCapabilityClient,
  context?: RequestContext,
): Promise<RingCapabilities> {
  const catalog = await readCatalog(client, context);
  return Object.freeze(
    Object.fromEntries(
      RING_FEATURES.map((feature) => [feature, capability(featureKeyNames(feature), catalog)]),
    ) as Record<RingFeature, RingCapability>,
  );
}

/** The key files a feature proves with, the ring keys in catalog order ahead of SPP's. */
export function featureKeyNames(feature: RingFeature): readonly string[] {
  const ring = RING_KEYS.filter(({ circuit }) => servesFeature(feature, circuit)).map(
    ({ name }) => `${name.replaceAll("-", "_")}.key`,
  );
  // SPP proves a ring merge on its `merge-ring` key of the padded width.
  return feature === "merge"
    ? [...ring, ...MERGE_SUPPORTED_INPUT_COUNTS.map((inputs) => `merge_ring_${inputs}_1.key`)]
    : ring;
}

/** A deposit spends no note, the narrow keys hold its frame. */
function servesFeature(feature: RingFeature, circuit: RingKeyCircuit): boolean {
  switch (feature) {
    case "transfer":
      return circuit.family === "transfer";
    case "deposit":
      return circuit.family === "transfer" && !circuit.entry && !circuit.wide;
    case "merge":
      return circuit.family === "merge";
    case "enroll":
      // The owner proves its RegisterKey and Ring RPC proves the EnrollBatch on an apply enrollments key.
      return circuit.family === "registerKey" || circuit.family === "applyEnrollments";
    case "applyRecords":
      return circuit.family === "applyRecords";
  }
}

type Catalog =
  | Readonly<{ kind: "served"; served: ReadonlySet<string> }>
  /** Every ring pin outside `missing` is served. */
  | Readonly<{ kind: "missing"; missing: ReadonlySet<string>; code: string }>
  | Readonly<{ kind: "mismatched"; mismatched: ReadonlySet<string>; code: string }>
  | Readonly<{ kind: "failed"; code: string }>;

async function readCatalog(
  client: RingCapabilityClient,
  context: RequestContext | undefined,
): Promise<Catalog> {
  try {
    const report = await client.checkRingProvingKeys(context);
    return { kind: "served", served: new Set(report.keys.map((key) => key.name)) };
  } catch (cause) {
    if (context?.signal?.aborted === true) throw cause;
    if (!(cause instanceof RingError) && !(cause instanceof ClientError)) throw cause;
    const { code } = cause;
    const named = cause.details?.["keyNames"];
    const keyNames = new Set(typeof named === "string" ? named.split(",") : []);
    if (code === "RING_PROVING_KEYS_MISSING" && keyNames.size > 0)
      return { kind: "missing", missing: keyNames, code };
    if (code === "CLIENT_PROVER_PROVING_KEYS_MISMATCH" && keyNames.size > 0)
      return { kind: "mismatched", mismatched: keyNames, code };
    return { kind: "failed", code };
  }
}

function capability(keyNames: readonly string[], catalog: Catalog): RingCapability {
  const unpinned = keyNames.filter((name) => !ringPinned(name) && !corePinned(name));
  if (unpinned.length > 0) return unavailable("unpinned", unpinned);
  switch (catalog.kind) {
    case "served": {
      const missing = keyNames.filter((name) => !catalog.served.has(name));
      return missing.length > 0 ? unavailable("missing", missing) : available(keyNames);
    }
    case "missing": {
      const missing = keyNames.filter((name) => catalog.missing.has(name));
      if (missing.length > 0) return unavailable("missing", missing);
      // The ring check lists only the ring pins it misses.
      const unchecked = keyNames.filter((name) => !ringPinned(name));
      return unchecked.length > 0
        ? unavailable("unverified", unchecked, catalog.code)
        : available(keyNames);
    }
    case "mismatched": {
      const mismatched = keyNames.filter((name) => catalog.mismatched.has(name));
      return mismatched.length > 0
        ? unavailable("mismatched", mismatched)
        : unavailable("unverified", keyNames, catalog.code);
    }
    case "failed":
      return unavailable("unverified", keyNames, catalog.code);
  }
}

function available(keyNames: readonly string[]): RingCapability {
  return Object.freeze({ available: true, keyNames: Object.freeze([...keyNames]) });
}

function unavailable(
  reason: RingCapabilityReason,
  keyNames: readonly string[],
  code?: string,
): RingCapability {
  return Object.freeze({
    available: false,
    reason,
    keyNames: Object.freeze([...keyNames]),
    ...defined({ code }),
  });
}

function ringPinned(name: string): boolean {
  return Object.hasOwn(RING_PROVING_KEY_SHA256S, name);
}

function corePinned(name: string): boolean {
  return Object.hasOwn(PROVING_KEY_SHA256S, name);
}
