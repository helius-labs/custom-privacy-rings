/** Rust `zolana_ring_indexer::tracking`. */
import {
  equalBytes,
  indexerCodec,
  indexerScalars,
  type MethodDescriptor,
} from "@heliuslabs/spr/internal";
import type { Address, Bytes32 } from "@heliuslabs/spr/interface";

import { RingError } from "../error.js";
import {
  QUAD_ARITY,
  QUAD_CAPACITY,
  QUAD_HEIGHT,
  type QuadPath,
} from "../membership/member-registry.js";
import {
  viewingKeyHash,
  type ServedTrackingPool,
  type TrackingOpening,
  type TrackingRecord,
} from "../membership/tracking.js";
import { MAX_RING_MEMBER_BATCH } from "./indexer-limits.js";

const { array, context, object, toWireInteger, wireInteger } = indexerCodec;
const { checkedAddress, checkedBase64, checkedHash, hash, hashBytes, schemaFailure } =
  indexerScalars;

const U64_MAX = (1n << 64n) - 1n;

export interface TrackingOpeningsRequest {
  readonly registry: Address;
  readonly members: readonly Bytes32[];
  readonly expectedRoot: Bytes32;
}

/** The inserting entry's seal beside the leaf's current `viewingU`, unauthenticated until `sealViewingKey`. */
export interface TrackingSeal {
  readonly owner: Address;
  readonly viewingU: Bytes32;
  readonly ephPk: Uint8Array;
  readonly ciphertext: Uint8Array;
}

/** Unverified until `verifyTrackingOpening` checks it under the bound root. */
export type ServedTrackingOpening = TrackingOpening & Readonly<{ seal?: TrackingSeal }>;

export interface TrackingOpenings {
  readonly slot: bigint;
  readonly root: Bytes32;
  readonly revision: bigint;
  readonly openings: readonly ServedTrackingOpening[];
}

export interface EnrollmentQueueRequest {
  readonly registry: Address;
  /** Exclusive, the page starts at `head` when absent. */
  readonly after?: bigint;
  readonly limit: number;
}

/** One `Enroll` no apply consumed, it grants nothing and resolves nothing. */
export interface QueuedEnrollment {
  readonly sequence: bigint;
  readonly owner: Address;
  readonly member: Bytes32;
  /** The `Enroll` verified `RegisterKey`, unset on a rotation. */
  readonly proven: boolean;
  readonly nullifierPk: Bytes32;
  readonly listTag: Bytes32;
  readonly viewingU: Bytes32;
  readonly ephPk: Uint8Array;
  readonly ciphertext: Uint8Array;
}

/** `head` and `tail` as the queue account held them at `slot`. */
export interface EnrollmentQueuePage {
  readonly slot: bigint;
  readonly head: bigint;
  readonly tail: bigint;
  readonly entries: readonly QueuedEnrollment[];
}

/** The seal's viewing key, refused unless its hash is the verified leaf's `viewingHash`. */
export function sealViewingKey(seal: TrackingSeal, record: TrackingRecord): Bytes32 {
  if (!equalBytes(viewingKeyHash(seal.viewingU), record.viewingHash))
    throw new RingError("RING_VIEWING_KEY_MISMATCH");
  return seal.viewingU;
}

export const getTrackingOpeningsMethod: MethodDescriptor<
  TrackingOpeningsRequest,
  TrackingOpenings
> = {
  name: "getTrackingOpenings",
  encodeRequest: (request) => {
    if (request.members.length < 1 || request.members.length > MAX_RING_MEMBER_BATCH)
      schemaFailure(
        "INDEXER_SCHEMA_INVALID_TYPE",
        "$.members",
        `1 to ${String(MAX_RING_MEMBER_BATCH)} members`,
        request.members,
      );
    return {
      registry: checkedAddress(request.registry, "registry"),
      members: request.members.map((member) => hash(member)),
      expectedRoot: hash(request.expectedRoot),
    };
  },
  decodeResponse: (value) => {
    const row = object(value, "$", ["context", "root", "openings"]);
    const root = object(row["root"], "root", ["root", "revision", "nextIndex"]);
    return Object.freeze({
      slot: context(row["context"], "context").slot,
      root: hashBytes(checkedHash(root["root"], "root.root")),
      revision: u64(root["revision"], "root.revision"),
      openings: Object.freeze(array(row["openings"], "openings", opening)),
    });
  },
};

export interface TrackingRecordPoolRequest extends TrackingOpeningsRequest {
  readonly openings: number;
}

export const getTrackingRecordPoolMethod: MethodDescriptor<
  TrackingRecordPoolRequest,
  ServedTrackingPool
> = {
  name: "getTrackingRecordPool",
  encodeRequest: (request) => ({
    ...getTrackingOpeningsMethod.encodeRequest(request),
    openings: request.openings,
  }),
  decodeResponse: (value) => {
    const row = object(value, "$", ["context", "root", "records", "canopy", "subjects"]);
    return Object.freeze({
      records: Object.freeze(
        array(row["records"], "records", (entry, path) => {
          const served = object(entry, path, ["record", "path"]);
          return Object.freeze({
            record: servedRecord(served["record"], `${path}.record`).record,
            path: poolPath(served["path"], `${path}.path`),
          });
        }),
      ),
      canopy: Object.freeze(array(row["canopy"], "canopy", field)),
      subjects: Object.freeze(
        array(row["subjects"], "subjects", (entry, path) =>
          Number(wireInteger(entry, path, 0n, 0xffff_ffffn)),
        ),
      ),
    });
  },
};

export const getEnrollmentQueueMethod: MethodDescriptor<
  EnrollmentQueueRequest,
  EnrollmentQueuePage
> = {
  name: "getEnrollmentQueue",
  encodeRequest: (request) => ({
    registry: checkedAddress(request.registry, "registry"),
    ...(request.after === undefined
      ? {}
      : { after: toWireInteger(request.after, "after", 0n, U64_MAX) }),
    limit: toWireInteger(BigInt(request.limit), "limit", 1n, BigInt(MAX_RING_MEMBER_BATCH)),
  }),
  decodeResponse: (value) => {
    const row = object(value, "$", ["context", "head", "tail", "entries"]);
    const head = u64(row["head"], "head");
    const tail = u64(row["tail"], "tail");
    if (tail < head)
      schemaFailure("INDEXER_SCHEMA_INVALID_TYPE", "$.tail", "at least head", row["tail"]);
    return Object.freeze({
      slot: context(row["context"], "context").slot,
      head,
      tail,
      entries: Object.freeze(array(row["entries"], "entries", queuedEnrollment)),
    });
  },
};

function opening(value: unknown, path: string): ServedTrackingOpening {
  const row = object(value, path, ["present", "record", "path"]);
  const present = row["present"];
  if (typeof present !== "boolean")
    return schemaFailure("INDEXER_SCHEMA_INVALID_TYPE", `${path}.present`, "a boolean", present);
  const { record, seal } = servedRecord(row["record"], `${path}.record`);
  const proofPath = quadPath(row["path"], `${path}.path`);
  return Object.freeze(
    present
      ? { kind: "present", record, path: proofPath, ...(seal === undefined ? {} : { seal }) }
      : { kind: "absent", low: record, path: proofPath },
  );
}

function servedRecord(
  value: unknown,
  path: string,
): Readonly<{ record: TrackingRecord; seal?: TrackingSeal }> {
  const row = object(value, path, [
    "member",
    "next",
    "nullifierPk",
    "ctHash",
    "listTag",
    "viewingHash",
    "seal",
  ]);
  const seal = row["seal"];
  return {
    record: Object.freeze({
      member: field(row["member"], `${path}.member`),
      next: field(row["next"], `${path}.next`),
      nullifierPk: field(row["nullifierPk"], `${path}.nullifierPk`),
      ctHash: field(row["ctHash"], `${path}.ctHash`),
      listTag: field(row["listTag"], `${path}.listTag`),
      viewingHash: field(row["viewingHash"], `${path}.viewingHash`),
    }),
    ...(seal == null ? {} : { seal: sealOf(seal, `${path}.seal`) }),
  };
}

function sealOf(value: unknown, path: string): TrackingSeal {
  const row = object(value, path, ["owner", "viewingU", "ephPk", "ciphertext"]);
  return Object.freeze({
    owner: checkedAddress(row["owner"], `${path}.owner`),
    viewingU: viewingKey(row["viewingU"], `${path}.viewingU`),
    ephPk: bytes(row["ephPk"], `${path}.ephPk`),
    ciphertext: bytes(row["ciphertext"], `${path}.ciphertext`),
  });
}

function quadPath(value: unknown, path: string): QuadPath {
  const read = poolPath(value, path);
  if (read.siblings.length !== QUAD_HEIGHT)
    return schemaFailure(
      "INDEXER_SCHEMA_INVALID_TYPE",
      `${path}.siblings`,
      `${String(QUAD_HEIGHT)} levels`,
      read.siblings,
    );
  return read;
}

/** A pool path stops at the canopy, `verifyTrackingPool` checks its height. */
function poolPath(value: unknown, path: string): QuadPath {
  const row = object(value, path, ["index", "siblings"]);
  const siblings = array(row["siblings"], `${path}.siblings`, (level, levelPath) => {
    const hashes = array(level, levelPath, field);
    if (hashes.length !== QUAD_ARITY - 1)
      return schemaFailure("INDEXER_SCHEMA_INVALID_TYPE", levelPath, "3 siblings", level);
    return Object.freeze(hashes);
  });
  return Object.freeze({
    index: Number(wireInteger(row["index"], `${path}.index`, 0n, QUAD_CAPACITY - 1n)),
    siblings: Object.freeze(siblings),
  });
}

function queuedEnrollment(value: unknown, path: string): QueuedEnrollment {
  const row = object(value, path, [
    "sequence",
    "owner",
    "member",
    "proven",
    "nullifierPk",
    "listTag",
    "viewingU",
    "ephPk",
    "ciphertext",
  ]);
  const proven = row["proven"];
  if (typeof proven !== "boolean")
    return schemaFailure("INDEXER_SCHEMA_INVALID_TYPE", `${path}.proven`, "a boolean", proven);
  return Object.freeze({
    sequence: u64(row["sequence"], `${path}.sequence`),
    owner: checkedAddress(row["owner"], `${path}.owner`),
    member: field(row["member"], `${path}.member`),
    proven,
    nullifierPk: field(row["nullifierPk"], `${path}.nullifierPk`),
    listTag: field(row["listTag"], `${path}.listTag`),
    viewingU: viewingKey(row["viewingU"], `${path}.viewingU`),
    ephPk: bytes(row["ephPk"], `${path}.ephPk`),
    ciphertext: bytes(row["ciphertext"], `${path}.ciphertext`),
  });
}

/** Base64, an X25519 `u` can exceed the field a hash holds. */
function viewingKey(value: unknown, path: string): Bytes32 {
  const key = bytes(value, path);
  if (key.length !== 32)
    return schemaFailure("INDEXER_SCHEMA_INVALID_TYPE", path, "32 bytes", value);
  return key as Bytes32;
}

function bytes(value: unknown, path: string): Uint8Array {
  return indexerScalars.base64Bytes(checkedBase64(value, path));
}

function field(value: unknown, path: string): Bytes32 {
  return hashBytes(checkedHash(value, path));
}

function u64(value: unknown, path: string): bigint {
  return wireInteger(value, path, 0n, U64_MAX);
}
