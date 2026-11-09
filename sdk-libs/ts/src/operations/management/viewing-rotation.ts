/**
 * A wallet's viewing key rotation through Ring RPC.
 *
 * Read after `operations/management/enrollment.ts`, whose request without a
 * proof carries the new key. Read after `LocalRingKeys` and `ViewingHistory`,
 * the two halves of a wallet a rotation moves together.
 *
 * A rotation runs in three steps. `rotateViewingKey` is local, and the wallet
 * stores the new viewing secret and the rotated history before the owner signs
 * what `updateViewingKey` builds. A leaf naming a key the wallet lost routes
 * every later payment to notes nobody opens. Ring RPC writes the signed request,
 * and senders read the new key only once an apply moves the owner's leaf to it.
 * `confirmViewingKey` reads that leaf back. The leaf holds only the current
 * key's hash, the history keeps every earlier key, and notes sealed to an
 * earlier key open through it.
 */
import type { Address } from "@solana/kit";

import { equalBytes } from "@heliuslabs/spr/internal";
import type { RequestContext } from "@heliuslabs/spr/interface";
import type { ViewingKey } from "@heliuslabs/spr/keypair";

import { RingError } from "../../protocol/error.js";
import { fetchRingPolicy } from "../../protocol/membership/config.js";
import type { RelayEnrollInputs } from "../../protocol/ring-rpc/relay-enroll-inputs.js";
import type { LocalRingKeys, RingKeys } from "../../state/ring-keys.js";
import type { ViewingHistory } from "../../state/viewing-history.js";
import {
  enrollRequestOf,
  enrollmentState,
  type EnrollmentReadClient,
  type EnrollmentState,
} from "./enrollment.js";

/** `rotateViewingKey` moves both to one current key. */
export interface ViewingRotation {
  readonly keys: LocalRingKeys;
  readonly history: ViewingHistory;
}

/** `next` becomes current in both, every earlier key stays held and scanned. */
export function rotateViewingKey(
  input: Readonly<ViewingRotation & { next: ViewingKey; rotatedAt: bigint }>,
): ViewingRotation {
  const history = input.history.withRotatedKey(input.next.publicKey(), input.rotatedAt);
  return Object.freeze({ keys: input.keys.rotate(input.next), history });
}

/** The proofless request of the holder's current key, `undefined` when the leaf already holds it. */
export async function updateViewingKey(
  input: Readonly<{
    client: EnrollmentReadClient;
    ringProgramId: Address;
    owner: Address;
    keys: RingKeys;
    history: ViewingHistory;
  }>,
  context?: RequestContext,
): Promise<RelayEnrollInputs | undefined> {
  // 1. No sync reads a key the history lacks. Publishing one routes payments
  // to notes the wallet never finds.
  const viewingU = input.keys.address().viewingPublicKey.u();
  if (!equalBytes(input.history.current().viewingPublicKey.u(), viewingU))
    throw new RingError("RING_VIEWING_HISTORY_NOT_CURRENT");
  // 2. A rotation repeats the seal of the first enrollment. A holder of another
  // nullifier key would publish a viewing key the enrolled leaf does not belong to.
  const { tracking } = await fetchRingPolicy(input.client, input.ringProgramId, context);
  const stored = await storedEnrollment({ ...input, trackingRegistry: tracking }, context);
  if (!equalBytes(stored.seal.nullifierPk, input.keys.address().nullifierPublicKey))
    throw new RingError("RING_ENROLLMENT_KEY_MISMATCH");
  if (equalBytes(stored.viewingU, viewingU)) return undefined;
  return enrollRequestOf(
    { ringProgramId: input.ringProgramId, owner: input.owner, seal: stored.seal, viewingU },
    tracking,
  );
}

/** Resolves once the leaf under the newest root holds the holder's current key, an absent leaf or an older key refuses. */
export async function confirmViewingKey(
  input: Readonly<{
    client: EnrollmentReadClient;
    owner: Address;
    trackingRegistry: Address;
    keys: RingKeys;
  }>,
  context?: RequestContext,
): Promise<EnrollmentState> {
  const stored = await enrollmentState(
    { trackingRegistry: input.trackingRegistry, owner: input.owner },
    input.client,
    context,
  );
  if (
    stored === undefined ||
    !equalBytes(stored.viewingU, input.keys.address().viewingPublicKey.u())
  )
    throw new RingError("RING_VIEWING_KEY_NOT_PUBLISHED", { details: { owner: input.owner } });
  return stored;
}

async function storedEnrollment(
  input: Readonly<{
    client: EnrollmentReadClient;
    owner: Address;
    trackingRegistry: Address;
  }>,
  context?: RequestContext,
): Promise<EnrollmentState> {
  const { owner } = input;
  const stored = await enrollmentState(
    { trackingRegistry: input.trackingRegistry, owner },
    input.client,
    context,
  );
  if (stored === undefined) throw new RingError("RING_KEY_NOT_REGISTERED", { details: { owner } });
  return stored;
}
