/**
 * A ring output owner's nullifier key for the auditor, read from the owner's tracking leaf under
 * the newest root of the ring's pinned registry. No proof reads it, the rebuilt commitment decides.
 * Read before `audit.ts`.
 */
import type { Address } from "@solana/kit";

import type { ChainReader } from "@heliuslabs/spr/client";
import { bytesKey } from "@heliuslabs/spr/internal";
import type { Bytes32, RequestContext } from "@heliuslabs/spr/interface";

import { RingError } from "../../protocol/error.js";
import { MAX_RING_MEMBER_BATCH } from "../../protocol/indexer/indexer-limits.js";
import { fetchRingPolicy } from "../../protocol/membership/config.js";
import { memberOfTag } from "../../protocol/membership/policy.js";
import { verifyTrackingOpening } from "../../protocol/membership/tracking.js";
import type { TrackingReader } from "../../rpc/ports.js";
import { newestTrackingRoot } from "../../rpc/tracking-resolver.js";

/** Names the nullifier key a ring output's record omits, `undefined` while unknown. */
export type OwnerNullifierKey = (ownerKey: Bytes32) => Bytes32 | undefined;

export type RegisteredKeysClient = Pick<ChainReader, "getAccount"> &
  Pick<TrackingReader, "getTrackingOpenings">;

/**
 * Rust `RegisteredKeys::read`, an owner absent under the newest root stays unresolved for a later
 * scan.
 */
export async function readRegisteredKeys(
  input: Readonly<{
    client: RegisteredKeysClient;
    ringProgramId: Address;
    owners: readonly Bytes32[];
  }>,
  context?: RequestContext,
): Promise<OwnerNullifierKey> {
  const { client, ringProgramId } = input;
  const owners = [
    ...new Map(input.owners.map((owner) => [bytesKey(owner), owner] as const)).values(),
  ];
  const keys = new Map<string, Bytes32>();
  if (owners.length > 0) {
    const { tracking } = await fetchRingPolicy(client, ringProgramId, context);
    const { root } = await newestTrackingRoot(
      { client, trackingRegistry: tracking, ring: ringProgramId },
      context,
    );
    for (let start = 0; start < owners.length; start += MAX_RING_MEMBER_BATCH) {
      const page = owners.slice(start, start + MAX_RING_MEMBER_BATCH);
      const members = page.map((owner) => memberOfTag(owner));
      const { openings } = await client.getTrackingOpenings(
        { registry: tracking, members, expectedRoot: root },
        context,
      );
      if (openings.length !== page.length)
        throw new RingError("RING_TRACKING_INVALID", { details: { reason: "openings" } });
      page.forEach((owner, index) => {
        const row = verifyTrackingOpening(
          openings[index] as (typeof openings)[number],
          root,
          members[index] as Bytes32,
        );
        if (row !== undefined) keys.set(bytesKey(owner), row.nullifierPk);
      });
    }
  }
  return (ownerKey) => keys.get(bytesKey(ownerKey));
}
