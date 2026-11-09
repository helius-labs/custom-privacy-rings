import type { Address } from "@solana/kit";

import type { ChainReader } from "@heliuslabs/spr/client";
import { equalBytes, waitForProjection, ZERO_NULLIFIER_PK } from "@heliuslabs/spr/internal";
import type { Bytes32, RequestContext } from "@heliuslabs/spr/interface";
import type { X25519PublicKey } from "@heliuslabs/spr/keypair";

import { listTagOf } from "../crypto/keys/list-salt.js";
import type { RingAddress } from "../crypto/keys/ring-address.js";
import { x25519FromU } from "../crypto/keys/x25519.js";
import type { StorageRegistryRoot } from "../protocol/codec/ring-codecs.js";
import { RingError } from "../protocol/error.js";
import { sealViewingKey, type ServedTrackingOpening } from "../protocol/indexer/tracking-codec.js";
import {
  decodeTrackingRegistry,
  verifyTrackingOpening,
  type TrackingRecord,
  type TrackingRegistry,
} from "../protocol/membership/tracking.js";
import { MEMBER_REGISTRY_PROJECTION_ERRORS } from "./projection.js";
import type { BoundRoot } from "./registered-record.js";
import type { TrackingReader } from "./ports.js";

export type RingRecipientClient = Pick<ChainReader, "getAccount"> &
  Pick<TrackingReader, "getTrackingOpenings">;

export type TrackingRoot = BoundRoot;

export type ResolvedRecipient =
  /** The leaf's viewing key, an address from before a rotation resolves to the current one, Rust `RecipientResolver`. */
  | Readonly<{
      kind: "enrolled";
      root: TrackingRoot;
      nullifierPk: Bytes32;
      viewingPublicKey: X25519PublicKey;
      listTag: Bytes32;
    }>
  /** Zero key resolution defers enrollment checks to payment validation. */
  | Readonly<{
      kind: "zeroKey";
      root: TrackingRoot;
      viewingPublicKey: X25519PublicKey;
    }>
  /**
   * A nonzero key with no row under `root`, read from the address with no list tag.
   * Only a later registration of the same key spends the note.
   */
  | Readonly<{
      kind: "unregistered";
      root: TrackingRoot;
      nullifierPk: Bytes32;
      viewingPublicKey: X25519PublicKey;
    }>;

/**
 * Zero key resolution defers enrollment checks to payment validation.
 *
 * A rotation applied after the read leaves the payment sealed to the key read, no program check
 * reads a viewing key.
 */
export async function resolveRingRecipient(
  input: Readonly<{
    client: RingRecipientClient;
    trackingRegistry: Address;
    to: RingAddress;
  }>,
  context?: RequestContext,
): Promise<ResolvedRecipient> {
  const { client, trackingRegistry, to } = input;
  const member = to.recipient.signingPublicKey.ownerProofInputHash();
  const read = await readTrackingOpenings({ client, trackingRegistry, members: [member] }, context);
  const root: TrackingRoot = Object.freeze({
    ringProgramId: to.ringProgramId,
    registry: trackingRegistry,
    revision: read.root.revision,
    root: read.root.hash,
  });
  const opening = read.openings[0] as ServedTrackingOpening;
  const row = verifyTrackingOpening(opening, root.root, member);
  const named = to.recipient.nullifierPublicKey;
  if (equalBytes(named, ZERO_NULLIFIER_PK)) {
    const viewingPublicKey =
      row === undefined ? to.recipient.viewingPublicKey : leafViewingKey(opening, row);
    return Object.freeze({ kind: "zeroKey", root, viewingPublicKey });
  }
  // A nonzero key pays an owner the opening proved absent under `root`.
  if (row === undefined)
    return Object.freeze({
      kind: "unregistered",
      root,
      nullifierPk: named,
      viewingPublicKey: to.recipient.viewingPublicKey,
    });
  if (!equalBytes(row.nullifierPk, named))
    throw new RingError("RING_ADDRESS_MISMATCH", {
      details: { field: "nullifierPk" },
    });
  if (!equalBytes(row.listTag, listTagOf(to.listSalt)))
    throw new RingError("RING_ADDRESS_MISMATCH", {
      details: { field: "listSalt" },
    });
  return Object.freeze({
    kind: "enrolled",
    root,
    nullifierPk: row.nullifierPk,
    viewingPublicKey: leafViewingKey(opening, row),
    listTag: row.listTag,
  });
}

/**
 * Rereads the root with the openings while the indexer lags an apply that moves the root, Rust
 * `RingError::is_projection_lag`.
 */
export function readTrackingOpenings(
  input: Readonly<{
    client: RingRecipientClient;
    trackingRegistry: Address;
    members: readonly Bytes32[];
  }>,
  context?: RequestContext,
): Promise<Readonly<{ root: StorageRegistryRoot; openings: readonly ServedTrackingOpening[] }>> {
  const { client, trackingRegistry, members } = input;
  return waitForProjection(
    async (attempt) => {
      const root = currentTrackingRoot(
        await fetchTrackingRegistry(client, trackingRegistry, attempt),
      );
      const { openings } = await client.getTrackingOpenings(
        { registry: trackingRegistry, members, expectedRoot: root.hash },
        attempt,
      );
      if (openings.length !== members.length)
        throw new RingError("RING_TRACKING_INVALID", {
          details: { reason: "openings", count: openings.length },
        });
      return Object.freeze({ root, openings });
    },
    MEMBER_REGISTRY_PROJECTION_ERRORS,
    context,
  );
}

export async function newestTrackingRoot(
  input: Readonly<{
    client: Pick<ChainReader, "getAccount">;
    trackingRegistry: Address;
    ring: Address;
  }>,
  context?: RequestContext,
): Promise<TrackingRoot> {
  const registry = await fetchTrackingRegistry(input.client, input.trackingRegistry, context);
  const newest = currentTrackingRoot(registry);
  return Object.freeze({
    ringProgramId: input.ring,
    registry: input.trackingRegistry,
    revision: newest.revision,
    root: newest.hash,
  });
}

/** The newest root, a registry with none refuses. */
export function currentTrackingRoot(registry: TrackingRegistry): StorageRegistryRoot {
  const newest = registry.roots[0];
  if (newest === undefined)
    throw new RingError("RING_TRACKING_INVALID", { details: { reason: "no root" } });
  return newest;
}

export async function fetchTrackingRegistry(
  client: Pick<ChainReader, "getAccount">,
  address: Address,
  context?: RequestContext,
): Promise<TrackingRegistry> {
  const account = await client.getAccount(address, context);
  if (account === undefined)
    throw new RingError("RING_TRACKING_INVALID", {
      details: { reason: "registry" },
    });
  return decodeTrackingRegistry(account.data);
}

/** A present leaf names the current viewing key, the address may predate a rotation. */
function leafViewingKey(opening: ServedTrackingOpening, record: TrackingRecord): X25519PublicKey {
  if (opening.kind !== "present" || opening.seal === undefined)
    throw new RingError("RING_TRACKING_INVALID", { details: { reason: "seal" } });
  return x25519FromU(sealViewingKey(opening.seal, record));
}
