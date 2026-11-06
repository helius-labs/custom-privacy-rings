/** The registries an instruction reads after the ring account and the tracking rows its proof opens. */
import type { Address } from "@solana/kit";

import { addressBytes, isZero } from "@heliuslabs/spr/internal";
import type { Bytes32, RequestContext } from "@heliuslabs/spr/interface";

import { registryCurator, type RingPolicy } from "../../protocol/codec/ring-codecs.js";
import { RingError } from "../../protocol/error.js";
import type { TrackingOpening } from "../../protocol/membership/tracking.js";
import { verifyTrackingOpening } from "../../protocol/membership/tracking.js";
import type { BoundRoot } from "../../rpc/registered-record.js";
import type { TrackingReader } from "../../rpc/ports.js";

/** Rust `RootRead`, policy roots only, every read also takes the pinned tracking registry. */
export type RootRead = "none" | "own" | "policy";

/** Rust `RootRead::transfer`, a delegate reads no policy root, a restrictive update cannot fail its proof. */
export function transferRootRead(
  config: Pick<RingPolicy, "hasPolicy" | "records">,
  delegate: boolean,
): RootRead {
  if (delegate) return "none";
  if (config.hasPolicy) return "policy";
  return isZeroAddress(config.records) ? "none" : "own";
}

/** Rust `RootRead::merge`, a merge runs no rules. */
export const MERGE_ROOT_READ: RootRead = "none";

/** Rust `RegistrySlots::load`, own and curator by `read`, then tracking. */
export function ringRegistrySlots(
  config: Pick<RingPolicy, "records" | "tracking" | "sources">,
  read: RootRead,
): readonly Address[] {
  const curator = read === "policy" ? registryCurator(config) : undefined;
  return Object.freeze([
    ...(read === "none" ? [] : [config.records]),
    ...(curator === undefined ? [] : [curator]),
    config.tracking,
  ]);
}

/** Each output owner's opening under `root`, any output key pays its owner, Rust `check_output_key`. */
export async function readOutputKeys(
  input: Readonly<{
    client: Pick<TrackingReader, "getTrackingOpenings">;
    root: BoundRoot;
    outputs: readonly Readonly<{ member: Bytes32; nullifierPk: Bytes32 }>[];
  }>,
  context?: RequestContext,
): Promise<readonly TrackingOpening[]> {
  const { client, root, outputs } = input;
  const { openings } = await client.getTrackingOpenings(
    {
      registry: root.registry,
      members: outputs.map((output) => output.member),
      expectedRoot: root.root,
    },
    context,
  );
  if (openings.length !== outputs.length)
    throw new RingError("RING_TRACKING_INVALID", { details: { reason: "openings" } });
  return Object.freeze(
    outputs.map(({ member }, index) => {
      const opening = openings[index] as TrackingOpening;
      verifyTrackingOpening(opening, root.root, member);
      return opening;
    }),
  );
}

function isZeroAddress(address: Address): boolean {
  return isZero(addressBytes(address));
}
