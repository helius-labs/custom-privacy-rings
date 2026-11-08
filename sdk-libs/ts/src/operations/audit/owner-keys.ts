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

/** Names the nullifier key a ring output's record omits, `undefined` while unknown. */
export type OwnerNullifierKey = (ownerKey: Bytes32) => Bytes32 | undefined;

/**
 * Rust `RegisteredKeys::read`, the authorization fixes its key at creation and an owner with no
 * account yet stays unresolved for a later scan.
 */
export async function readRegisteredKeys(
  input: Readonly<{
    client: AuthorizationReader;
    ringProgramId: Address;
    owners: readonly Bytes32[];
  }>,
  context?: RequestContext,
): Promise<OwnerNullifierKey> {
  const owners = [...new Set(input.owners.map((owner) => encodeBase58(owner)))];
  const keys = new Map<Address, Bytes32>();
  if (owners.length > 0) {
    const trackingRegistry = await pinnedTrackingRegistry(input, context);
    for (const owner of owners) {
      const authorization = await readTrackingAuthorization(
        { client: input.client, trackingRegistry, owner },
        context,
      );
      if (authorization !== undefined) keys.set(owner, authorization.nullifierPk);
    }
  }
  return (ownerKey) => keys.get(encodeBase58(ownerKey));
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
