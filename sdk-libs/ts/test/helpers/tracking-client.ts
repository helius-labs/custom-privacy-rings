import type { Address } from "@solana/kit";
import { X25519Keypair } from "@heliuslabs/spr/keypair";

import type { EnrollmentReadClient } from "../../src/operations/management/enrollment.js";
import type { QueuedEnrollment } from "../../src/protocol/indexer/tracking-codec.js";
import { trackingTree, type FixtureTrackingEntry } from "./member-registry.js";
import { enrollmentQueueAccount, trackingRegistryAccount } from "./ring-accounts.js";

/** Read on every call, a test moves the tree by editing the arrays. */
export interface TrackingFixture {
  readonly applied: FixtureTrackingEntry[];
  readonly pending: QueuedEnrollment[];
  /** Queue entries the account holds, `pending.length` when absent. */
  readonly queued?: number;
}

/** The registry, its queue and the indexer over the fixture's leaves and queued entries. */
export function trackingClient(registry: Address, fixture: TrackingFixture): EnrollmentReadClient {
  const auditor = X25519Keypair.generate().publicKey().toBytes();
  const tree = () => trackingTree(fixture.applied);
  return {
    getAccount: async (key: Address) => {
      const [queue, account] = await enrollmentQueueAccount({
        registry,
        entries: Array.from(
          { length: fixture.queued ?? fixture.pending.length },
          () => new Uint8Array(32),
        ),
      });
      if (key === queue) return account;
      if (key !== registry) return undefined;
      const { root, nextIndex } = tree();
      return trackingRegistryAccount({ root, revision: 1n, nextIndex, auditorPubkey: auditor });
    },
    getTrackingOpenings: async (request) => {
      const tracked = tree();
      return {
        slot: 1n,
        root: tracked.root,
        revision: 1n,
        openings: request.members.map((member) => tracked.opening(member)),
      };
    },
    getEnrollmentQueue: async (request) => ({
      slot: 1n,
      head: 0n,
      tail: BigInt(fixture.pending.length),
      entries: fixture.pending
        .filter(({ sequence }) => request.after === undefined || sequence > request.after)
        .slice(0, request.limit),
    }),
  };
}
