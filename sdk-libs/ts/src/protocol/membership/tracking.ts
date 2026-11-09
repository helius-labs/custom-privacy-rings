/** The per-ring tracking registry, Rust `custom_ring_interface::tracking`. */
import { hexToBytes } from "@noble/hashes/utils.js";

import type { Address } from "@solana/kit";

import {
  bigIntBytes,
  checkedBytes,
  encodeBase58,
  equalBytes,
  hashBytes,
  poseidon,
  Reader,
  ZERO_32,
} from "@heliuslabs/spr/internal";
import type { Bytes32, Bytes33, Bytes64 } from "@heliuslabs/spr/interface";

import { domainField, RingDomain } from "../../crypto/derivation/ring-domain.js";
import { RingError } from "../error.js";
import type { StorageRegistryRoot } from "../codec/ring-codecs.js";
import { coveredPaths, foldQuadPath, type PoolSubject, type QuadPath } from "./member-registry.js";
import { FIELD_MAX } from "./member-opening.js";

export const TRACKING_GRACE_SLOTS = 750n;
/** Rust `TRACKING_QUAD_HEIGHT`. */
export const TRACKING_QUAD_HEIGHT = 16;
/** Rust `TRACKING_CAPACITY`. */
export const TRACKING_CAPACITY = 1n << BigInt(2 * TRACKING_QUAD_HEIGHT);
/** Rust `TRACKING_ROOT_HISTORY`. */
export const TRACKING_ROOT_HISTORY = 128;
/** The sentinel at leaf 0 under zero subtrees. */
export const TRACKING_EMPTY_ROOT = hexToBytes(
  "197ce7e4cc6e23ed6f4d5b32303dd91e032f9afdc7f8dd424ab2638d245e4559",
) as Bytes32;

const REGISTRY_DISCRIMINATOR = 3;
const OWNER_GRANT_DISCRIMINATOR = 5;
const REGISTRY_SIZE = 106 + 48 * TRACKING_ROOT_HISTORY;
const OWNER_GRANT_SIZE = 2;

export interface TrackingRegistry {
  /** The ring's `ring_auth`, the one signer of `ApplyEnrollments`. */
  readonly authority: Address;
  readonly ringId: Bytes32;
  readonly auditorPubkey: Bytes33;
  /** At most `TRACKING_CAPACITY`. */
  readonly nextIndex: bigint;
  /** Newest first. */
  readonly roots: readonly (StorageRegistryRoot | undefined)[];
}

/** The address names the registry, the owner and the reader. */
export interface OwnerGrant {
  readonly active: boolean;
}

export interface TrackingFields {
  readonly member: Bytes32;
  readonly nullifierPk: Bytes32;
  readonly ctHash: Bytes32;
  readonly listTag: Bytes32;
  /** `hash_bytes(viewing_u)`, an X25519 `u` can exceed the field. */
  readonly viewingHash: Bytes32;
}

/** One enrollment an apply chains, Rust `TrackingEntry`. */
export interface TrackingEntry extends TrackingFields {
  /** The leaf's viewing hash a Rotate replaces, zero on an Enroll. */
  readonly priorViewingHash: Bytes32;
  /** Set when the write carried a verified `RegisterKey` proof, only a proven entry inserts. */
  readonly proven: boolean;
}

export interface TrackingRecord extends TrackingFields {
  readonly next: Bytes32;
}

/** A member under one root, its row or the low row bracketing its absence. */
export type TrackingOpening =
  | Readonly<{ kind: "present"; record: TrackingRecord; path: QuadPath }>
  | Readonly<{ kind: "absent"; low: TrackingRecord; path: QuadPath }>;

export const TRACKING_SENTINEL: TrackingRecord = Object.freeze({
  member: ZERO_32,
  next: FIELD_MAX,
  nullifierPk: ZERO_32,
  ctHash: ZERO_32,
  listTag: ZERO_32,
  viewingHash: ZERO_32,
});

export function decodeTrackingRegistry(data: Uint8Array): TrackingRegistry {
  const reader = account(data, REGISTRY_DISCRIMINATOR, REGISTRY_SIZE);
  const authority = encodeBase58(reader.bytes(32, "authority"));
  const ringId = reader.bytes(32, "ringId") as Bytes32;
  const auditorPubkey = reader.bytes(33, "auditorPubkey") as Bytes33;
  const nextIndex = reader.u64("nextIndex");
  const roots = Array.from({ length: TRACKING_ROOT_HISTORY }, () => {
    const hash = reader.bytes(32, "root.hash") as Bytes32;
    const revision = reader.u64("root.revision");
    const expiresAt = reader.u64("root.expiresAt");
    return equalBytes(hash, ZERO_32) ? undefined : Object.freeze({ hash, revision, expiresAt });
  });
  reader.done();
  if (nextIndex < 1n || nextIndex > TRACKING_CAPACITY)
    throw new RingError("RING_TRACKING_INVALID", { details: { nextIndex: nextIndex.toString() } });
  return Object.freeze({
    authority,
    ringId,
    auditorPubkey,
    nextIndex,
    roots: Object.freeze(roots),
  });
}

export function decodeOwnerGrant(data: Uint8Array): OwnerGrant {
  const reader = account(data, OWNER_GRANT_DISCRIMINATOR, OWNER_GRANT_SIZE);
  const grant = Object.freeze({
    active: reader.u8("active") === 1,
  });
  reader.done();
  return grant;
}

/** Rust `TrackingEntry::from_enroll`, proven since every Enroll carries a `RegisterKey` proof. */
export function trackingEntryOf(
  input: Readonly<{
    member: Bytes32;
    nullifierPk: Bytes32;
    listTag: Bytes32;
    viewingU: Bytes32;
    ciphertext: Bytes64;
  }>,
): TrackingEntry {
  return Object.freeze({
    member: input.member,
    nullifierPk: input.nullifierPk,
    ctHash: hashBytes(checkedBytes<Bytes64>(input.ciphertext, 64, "ciphertext")) as Bytes32,
    listTag: input.listTag,
    viewingHash: viewingKeyHash(input.viewingU),
    priorViewingHash: ZERO_32,
    proven: true,
  });
}

/** Rust `TrackingEntry::from_rotation`, moves a present leaf's viewing hash only from `priorViewingHash`. */
export function trackingRotationOf(
  input: Readonly<{ member: Bytes32; viewingU: Bytes32; priorViewingHash: Bytes32 }>,
): TrackingEntry {
  return Object.freeze({
    member: input.member,
    nullifierPk: ZERO_32,
    ctHash: ZERO_32,
    listTag: ZERO_32,
    viewingHash: viewingKeyHash(input.viewingU),
    priorViewingHash: checkedBytes<Bytes32>(input.priorViewingHash, 32, "prior viewing hash"),
    proven: false,
  });
}

/** `hash_bytes(viewing_u)`, the leaf field a sender checks a served viewing key against. */
export function viewingKeyHash(viewingU: Bytes32): Bytes32 {
  return hashBytes(checkedBytes<Bytes32>(viewingU, 32, "viewing key")) as Bytes32;
}

/** `Poseidon(CRTQ, member, nullifier_pk, ct_hash, list_tag, viewing_hash, prior_viewing_hash, proven)`. */
export function trackingEntryHash(entry: TrackingEntry): Bytes32 {
  return poseidon([
    domainField(RingDomain.trackingEntry),
    ...trackingFields(entry),
    checkedBytes<Bytes32>(entry.priorViewingHash, 32, "prior viewing hash"),
    bigIntBytes(entry.proven ? 1n : 0n) as Bytes32,
  ]) as Bytes32;
}

/** `Poseidon(CRTK, member, next, nullifier_pk, ct_hash, list_tag, viewing_hash)`. */
export function trackingRecordHash(record: TrackingRecord): Bytes32 {
  const [member, ...keys] = trackingFields(record);
  return poseidon([
    domainField(RingDomain.trackingLeaf),
    member as Bytes32,
    checkedBytes<Bytes32>(record.next, 32, "next"),
    ...keys,
  ]) as Bytes32;
}

/** The member's row under `root`, `undefined` proves it unenrolled there. */
export function verifyTrackingOpening(
  opening: TrackingOpening,
  root: Bytes32,
  member: Bytes32,
): TrackingRecord | undefined {
  const leaf = opening.kind === "present" ? opening.record : opening.low;
  const held =
    opening.kind === "present"
      ? equalBytes(leaf.member, member)
      : fieldBelow(leaf.member, member) && fieldBelow(member, leaf.next);
  if (!held) throw invalid("NotBracketed");
  if (
    !equalBytes(
      foldQuadPath(opening.path, trackingRecordHash(leaf), refuse, TRACKING_QUAD_HEIGHT),
      root,
    )
  )
    throw invalid("RootMismatch");
  return opening.kind === "present" ? opening.record : undefined;
}

/** A leaf the indexer served, its path cut at the canopy. */
export interface ServedTrackingRecord {
  readonly record: TrackingRecord;
  readonly path: QuadPath;
}

/** Rust `GetTrackingRecordPoolResponse`, unverified until `verifyTrackingPool` checks it. */
export interface ServedTrackingPool {
  /** Distinct leaves in first use order. */
  readonly records: readonly ServedTrackingRecord[];
  readonly canopy: readonly Bytes32[];
  /** `subjects[i]` indexes the record holding or bracketing member `i`. */
  readonly subjects: readonly number[];
}

/** A tracking pool at its capacity, records and subjects repeating cyclically. */
export interface VerifiedTrackingPool {
  readonly records: readonly TrackingRecord[];
  readonly canopy: readonly Bytes32[];
  readonly paths: readonly QuadPath[];
  readonly subjects: readonly PoolSubject[];
  /** Each member's row in member order, `undefined` proves it unenrolled under the root. */
  readonly rows: readonly (TrackingRecord | undefined)[];
}

/** Every member's read under `root`, each pool path folding through the canopy into `root`. */
export function verifyTrackingPool(
  input: Readonly<{
    pool: ServedTrackingPool;
    root: Bytes32;
    members: readonly Bytes32[];
    openings: number;
  }>,
): VerifiedTrackingPool {
  const { pool, root, members, openings } = input;
  if (
    pool.records.length < 1 ||
    pool.records.length > openings ||
    pool.subjects.length !== members.length ||
    members.length > openings
  )
    throw invalid("PoolShape");
  const fullPaths = coveredPaths(
    {
      paths: pool.records.map(({ path }) => path),
      canopy: pool.canopy,
      openings,
      root,
      height: TRACKING_QUAD_HEIGHT,
    },
    refuse,
  );
  const rows = members.map((member, subject) => {
    const at = pool.subjects[subject];
    const served = at === undefined ? undefined : pool.records[at];
    const path = at === undefined ? undefined : fullPaths[at];
    if (served === undefined || path === undefined) throw invalid("SubjectIndex");
    const opening: TrackingOpening = equalBytes(served.record.member, member)
      ? { kind: "present", record: served.record, path }
      : { kind: "absent", low: served.record, path };
    return verifyTrackingOpening(opening, root, member);
  });
  const records = cyclic(pool.records, openings);
  return Object.freeze({
    records: Object.freeze(records.map(({ record }) => record)),
    canopy: Object.freeze([...pool.canopy]),
    paths: Object.freeze(records.map(({ path }) => path)),
    subjects: Object.freeze(
      cyclic(
        members.map((member, index) =>
          Object.freeze({ member, recordIndex: pool.subjects[index] ?? 0 }),
        ),
        openings,
      ),
    ),
    rows: Object.freeze(rows),
  });
}

function cyclic<T>(values: readonly T[], length: number): readonly T[] {
  return Array.from({ length }, (_, index) => values[index % values.length] as T);
}

function trackingFields(fields: TrackingFields): readonly Bytes32[] {
  return [
    checkedBytes<Bytes32>(fields.member, 32, "member"),
    checkedBytes<Bytes32>(fields.nullifierPk, 32, "nullifier pk"),
    checkedBytes<Bytes32>(fields.ctHash, 32, "ct hash"),
    checkedBytes<Bytes32>(fields.listTag, 32, "list tag"),
    checkedBytes<Bytes32>(fields.viewingHash, 32, "viewing hash"),
  ];
}

function fieldBelow(left: Bytes32, right: Bytes32): boolean {
  for (let at = 0; at < 32; at += 1) {
    const difference = (left[at] ?? 0) - (right[at] ?? 0);
    if (difference !== 0) return difference < 0;
  }
  return false;
}

function account(data: Uint8Array, discriminator: number, size: number): Reader {
  if (data.length !== size || data[0] !== discriminator)
    throw new RingError("RING_TRACKING_INVALID", {
      details: { length: data.length, discriminator: data[0] },
    });
  const reader = new Reader(data);
  reader.u8("discriminator");
  return reader;
}

function invalid(reason: string): RingError {
  return new RingError("RING_TRACKING_INVALID", { details: { reason } });
}

function refuse(reason: string): never {
  throw invalid(reason);
}
