/**
 * A ring output owner's nullifier key for the auditor, read from the owner's tracking authorization
 * under the ring's pinned registry. No proof reads it, the rebuilt commitment decides.
 * Read before `audit.ts`.
 */
import { getBase64Encoder, type Address } from "@solana/kit";

import { encodeBase58, runKitRpc } from "@heliuslabs/spr/internal";
import type { Bytes32, RequestContext } from "@heliuslabs/spr/interface";

import { decodeRingPolicy } from "../../protocol/codec/ring-codecs.js";
import { RingError } from "../../protocol/error.js";
import { ringPolicyAddress } from "../../protocol/pda/index.js";
import {
  readTrackingAuthorization,
  type AuthorizationReader,
} from "../../rpc/tracking-resolver.js";

/** The nullifier key a ring output's record omits, named by its owner. */
export interface OwnerNullifierKeys {
  /** `undefined` while no key is known for the owner. */
  nullifierKey(ownerKey: Bytes32): Bytes32 | undefined;
}

/** The authorization fixes its key at creation, a lookup miss reads again at the next `refresh`. */
export class TrackingOwnerKeys implements OwnerNullifierKeys {
  readonly #keys = new Map<Address, Bytes32>();
  readonly #missing = new Set<Address>();

  nullifierKey(ownerKey: Bytes32): Bytes32 | undefined {
    const owner = encodeBase58(ownerKey);
    const key = this.#keys.get(owner);
    if (key === undefined) this.#missing.add(owner);
    return key;
  }

  /** True when an owner missed since the last call now has an authorization. */
  async refresh(
    input: Readonly<{ client: AuthorizationReader; ringProgramId: Address }>,
    context?: RequestContext,
  ): Promise<boolean> {
    if (this.#missing.size === 0) return false;
    const owners = [...this.#missing];
    this.#missing.clear();
    const trackingRegistry = await pinnedTrackingRegistry(input, context);
    let found = false;
    for (const owner of owners) {
      const authorization = await readTrackingAuthorization(
        { client: input.client, trackingRegistry, owner },
        context,
      );
      if (authorization === undefined) continue;
      this.#keys.set(owner, authorization.nullifierPk);
      found = true;
    }
    return found;
  }
}

/** The policy account's tracking registry, an account the ring program does not own is refused. */
async function pinnedTrackingRegistry(
  input: Readonly<{ client: AuthorizationReader; ringProgramId: Address }>,
  context: RequestContext | undefined,
): Promise<Address> {
  const address = await ringPolicyAddress(input.ringProgramId);
  const { value: account } = await runKitRpc("getAccountInfo", context, (abortSignal) =>
    input.client.solanaRpc
      .getAccountInfo(address, { commitment: "confirmed", encoding: "base64" })
      .send({ abortSignal }),
  );
  if (account === null || account.owner !== input.ringProgramId)
    throw new RingError("RING_POLICY_INVALID", { details: { reason: "policy account" } });
  return decodeRingPolicy(getBase64Encoder().encode(account.data[0]) as Uint8Array).tracking;
}
