import type { Address } from "@solana/kit";
import { X25519Keypair } from "@heliuslabs/spr/keypair";

import type { EnrollmentReadClient } from "../../src/operations/management/enrollment.js";
import { trackingTree, type FixtureTrackingEntry } from "./member-registry.js";
import { ringPolicyPda } from "../../src/protocol/pda/index.js";
import { ownedAccount, ringPolicyData, trackingRegistryAccount } from "./ring-accounts.js";

/** The registry and the indexer over `applied`, read on every call, so a test moves the tree by editing it. */
export function trackingClient(
  registry: Address,
  applied: FixtureTrackingEntry[],
  accounts: ReadonlyMap<
    Address,
    Awaited<ReturnType<EnrollmentReadClient["getAccount"]>>
  > = new Map(),
): EnrollmentReadClient {
  const auditor = X25519Keypair.generate().publicKey().toBytes();
  return {
    getAccount: async (key: Address) => {
      if (key !== registry) return accounts.get(key);
      const { root, nextIndex } = trackingTree(applied);
      return trackingRegistryAccount({ root, revision: 1n, nextIndex, auditorPubkey: auditor });
    },
    getTrackingOpenings: async (request) => {
      const tracked = trackingTree(applied);
      return {
        slot: 1n,
        root: tracked.root,
        revision: 1n,
        openings: request.members.map((member) => tracked.opening(member)),
      };
    },
  };
}

/** `trackingClient` beside the policy of `ringProgramId` pinning `registry`. */
export async function ringTrackingClient(
  ringProgramId: Address,
  registry: Address,
  applied: FixtureTrackingEntry[],
): Promise<EnrollmentReadClient> {
  const [policy, bump] = await ringPolicyPda(ringProgramId);
  const account = ownedAccount(ringProgramId, ringPolicyData({ bump, tracking: registry }));
  return trackingClient(registry, applied, new Map([[policy, account]]));
}
