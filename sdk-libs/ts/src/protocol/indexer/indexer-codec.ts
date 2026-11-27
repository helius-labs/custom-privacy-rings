import { ASSET_LIMIT_KEYS, type MintPolicy } from "../membership/mint-policy.js";
import { indexerCodec, indexerScalars, type MethodDescriptor } from "@heliuslabs/spr/internal";
import type { Bytes32 } from "@heliuslabs/spr";

import {
  QUAD_CAPACITY,
  QUAD_ARITY,
  QUAD_HEIGHT,
  canopyLevels,
  type MemberRecord,
  type QuadPath,
  type RingMemberOpening,
  type RingServedRecord,
} from "../membership/member-registry.js";
import { schemaWire, toU64 } from "../wire-primitives.js";
import type {
  RingRecordWriteView,
  StorageApplyProofInputsRequest,
  StorageApplyProofInputsResponse,
  StorageMemberOpenings,
  StorageMemberPage,
  StorageOpeningsRequest,
  StoragePageRequest,
  StorageRecordPool,
  StorageRecordPoolRequest,
  StorageRoot,
} from "./indexer-types.js";

/** `spr_indexer_api::MAX_RING_MEMBER_PAGE_LIMIT`: members one ring page or opening batch names. */
export const MAX_RING_MEMBER_BATCH = 100;

const { array, context, object, toWireInteger, wireInteger } = indexerCodec;
const { checkedAddress, checkedHash, hash, hashBytes, schemaFailure } = indexerScalars;

type WireObject = Record<string, unknown>;

const { u64 } = schemaWire;

function boolean(value: unknown, path: string): boolean {
  if (typeof value !== "boolean") {
    return schemaFailure("INDEXER_SCHEMA_INVALID_TYPE", path, "a boolean", value);
  }
  return value;
}

function field(value: unknown, path: string): Bytes32 {
  return hashBytes(checkedHash(value, path));
}

/** Rust `GetStorageMemberOpeningsRequest` and `GetStorageMemberOpeningsResponse`. */
export const getStorageMemberOpeningsMethod: MethodDescriptor<
  StorageOpeningsRequest,
  StorageMemberOpenings
> = {
  name: "getStorageMemberOpenings",
  encodeRequest: openingsRequest,
  decodeResponse: (value) => {
    const row = object(value, "$", ["context", "root", "openings"]);
    return Object.freeze({
      context: context(row["context"], "context"),
      root: storageRoot(row["root"], "root"),
      openings: Object.freeze(array(row["openings"], "openings", memberOpening)),
    });
  },
};

/** Rust `GetStorageMemberRecordsRequest` and `GetStorageMemberRecordsResponse`. */
export const getStorageMemberRecordsMethod: MethodDescriptor<
  StoragePageRequest,
  StorageMemberPage
> = {
  name: "getStorageMemberRecords",
  encodeRequest: (value) => ({
    registry: checkedAddress(value.registry, "registry"),
    expectedRoot: hash(value.expectedRoot),
    startIndex: toWireInteger(BigInt(value.startIndex), "startIndex", 1n, QUAD_CAPACITY),
    limit: toWireInteger(BigInt(value.limit), "limit", 1n, BigInt(MAX_RING_MEMBER_BATCH)),
  }),
  decodeResponse: (value) => {
    const row = object(value, "$", ["context", "root", "records", "nextLeafIndex"]);
    const root = storageRoot(row["root"], "root");
    const records = servedRecords(row["records"], QUAD_HEIGHT);
    const nextLeafIndex = row["nextLeafIndex"];
    return Object.freeze({
      context: context(row["context"], "context"),
      root,
      records,
      nextLeafIndex:
        nextLeafIndex === null
          ? undefined
          : Number(wireInteger(nextLeafIndex, "nextLeafIndex", 1n, root.nextIndex)),
    });
  },
};

/** Rust `GetStorageRecordPoolRequest` and `GetStorageRecordPoolResponse`. */
export const getStorageRecordPoolMethod: MethodDescriptor<
  StorageRecordPoolRequest,
  StorageRecordPool
> = {
  name: "getStorageRecordPool",
  encodeRequest: (value) => {
    const wire = openingsRequest(value);
    const openings = toWireInteger(
      BigInt(value.openings),
      "openings",
      1n,
      BigInt(MAX_RING_MEMBER_BATCH),
    );
    if (value.members.length > openings)
      return schemaFailure(
        "INDEXER_SCHEMA_INVALID_TYPE",
        "$.members",
        `at most ${String(openings)} members`,
        value.members,
      );
    return { ...wire, openings };
  },
  decodeResponse: (value) => {
    const row = object(value, "$", ["context", "root", "records", "canopy", "subjects"]);
    const canopy = array(row["canopy"], "canopy", field);
    const levels = canopyLevels(canopy.length, QUAD_HEIGHT);
    if (canopy.length !== QUAD_ARITY ** levels)
      return schemaFailure("INDEXER_SCHEMA_INVALID_TYPE", "canopy", "a power of 4", row["canopy"]);
    const records = servedRecords(row["records"], QUAD_HEIGHT - levels);
    return Object.freeze({
      context: context(row["context"], "context"),
      root: storageRoot(row["root"], "root"),
      records,
      canopy: Object.freeze(canopy),
      subjects: Object.freeze(
        array(row["subjects"], "subjects", (index, path) =>
          Number(wireInteger(index, path, 0n, BigInt(MAX_RING_MEMBER_BATCH - 1))),
        ),
      ),
    });
  },
};

function openingsRequest(value: StorageOpeningsRequest): WireObject {
  const members: unknown = value.members;
  if (!Array.isArray(members) || members.length < 1 || members.length > MAX_RING_MEMBER_BATCH)
    return schemaFailure(
      "INDEXER_SCHEMA_INVALID_TYPE",
      "$.members",
      `1 to ${String(MAX_RING_MEMBER_BATCH)} members`,
      members,
    );
  return {
    registry: checkedAddress(value.registry, "registry"),
    members: value.members.map((member) => hash(member)),
    expectedRoot: hash(value.expectedRoot),
  };
}

function memberRecord(value: unknown, path: string): MemberRecord {
  const record = object(value, path, ["member", "next", "lists", "mintPolicy"]);
  return Object.freeze({
    member: field(record["member"], `${path}.member`),
    next: field(record["next"], `${path}.next`),
    lists: u64(record["lists"], `${path}.lists`),
    mintPolicy: decodeMintPolicy(record["mintPolicy"]),
  });
}

/** `index` then `levels` levels of 3 siblings, leaf level first. */
function recordPath(value: unknown, path: string, levels = QUAD_HEIGHT): QuadPath {
  const row = object(value, path, ["index", "siblings"]);
  const siblings = array(row["siblings"], `${path}.siblings`, (level, levelPath) => {
    const hashes = array(level, levelPath, field);
    if (hashes.length !== QUAD_ARITY - 1)
      return schemaFailure("INDEXER_SCHEMA_INVALID_TYPE", levelPath, "3 siblings", level);
    return Object.freeze(hashes);
  });
  if (siblings.length !== levels)
    return schemaFailure(
      "INDEXER_SCHEMA_INVALID_TYPE",
      `${path}.siblings`,
      `${String(levels)} levels`,
      row["siblings"],
    );
  return Object.freeze({
    index: Number(wireInteger(row["index"], `${path}.index`, 0n, QUAD_CAPACITY - 1n)),
    siblings: Object.freeze(siblings),
  });
}

function storageRoot(value: unknown, path: string): StorageRoot {
  const row = object(value, path, ["root", "revision", "nextIndex"]);
  return Object.freeze({
    root: field(row["root"], `${path}.root`),
    revision: u64(row["revision"], `${path}.revision`),
    nextIndex: wireInteger(row["nextIndex"], `${path}.nextIndex`, 1n, QUAD_CAPACITY),
  });
}

function memberOpening(value: unknown, path: string): RingMemberOpening {
  const row = object(value, path, ["present", "record", "path"]);
  const present = boolean(row["present"], `${path}.present`);
  const record = memberRecord(row["record"], `${path}.record`);
  const proofPath = recordPath(row["path"], `${path}.path`);
  return Object.freeze(
    present
      ? { kind: "present", record, path: proofPath }
      : { kind: "absent", low: record, path: proofPath },
  );
}

/** At most `MAX_RING_MEMBER_BATCH` leaves, each path `levels` deep. */
function servedRecords(value: unknown, levels: number): readonly RingServedRecord[] {
  const records = array(value, "records", (entry, path) => {
    const item = object(entry, path, ["record", "path"]);
    return Object.freeze({
      record: memberRecord(item["record"], `${path}.record`),
      path: recordPath(item["path"], `${path}.path`, levels),
    });
  });
  if (records.length > MAX_RING_MEMBER_BATCH)
    return schemaFailure(
      "INDEXER_SCHEMA_INVALID_TYPE",
      "records",
      `at most ${String(MAX_RING_MEMBER_BATCH)}`,
      value,
    );
  return Object.freeze(records);
}

/** Rust `GetStorageApplyProofInputsRequest`, each write in Rust `StorageWriteView` order. */
export function encodeStorageApplyProofInputsRequest(
  value: StorageApplyProofInputsRequest,
): WireObject {
  return {
    registry: checkedAddress(value.registry, "registry"),
    writes: value.writes.map((write, index) => {
      const path = `$.writes[${String(index)}]`;
      return {
        member: checkedHash(write.member, `${path}.member`),
        mask: toU64(write.mask, `${path}.mask`),
        value: toU64(write.value, `${path}.value`),
        mintPolicy: encodeMintPolicy(write.mintPolicy),
      };
    }),
  };
}

/** Rust `MintPolicy` in its field order, `null` keeps the stored settings. */
export function encodeMintPolicy(policy: MintPolicy | null): WireObject | null {
  return policy === null
    ? null
    : {
        threshold: toU64(policy.threshold, "threshold"),
        cap: toU64(policy.cap, "cap"),
        cosignAbove: toU64(policy.cosignAbove, "cosignAbove"),
        withdrawalThreshold: toU64(policy.withdrawalThreshold, "withdrawalThreshold"),
        limits: Object.fromEntries(
          ASSET_LIMIT_KEYS.map((key) => [key, toU64(policy.limits[key], `limits.${key}`)]),
        ),
      };
}

/** Rust `GetStorageApplyProofInputsResponse`. */
export function decodeStorageApplyProofInputs(value: unknown): StorageApplyProofInputsResponse {
  const row = object(value, "$", ["revision", ...APPLY_FIELDS]);
  return { ...applyProofInputs(row), revision: u64(row["revision"], "revision") };
}

const APPLY_FIELDS = [
  "context",
  "oldRoot",
  "newRoot",
  "oldNextIndex",
  "newNextIndex",
  "chainStart",
  "chainEnd",
  "writes",
] as const;

function applyProofInputs(row: WireObject): Omit<StorageApplyProofInputsResponse, "revision"> {
  return {
    context: context(row["context"], "context"),
    oldRoot: checkedHash(row["oldRoot"], "oldRoot"),
    newRoot: checkedHash(row["newRoot"], "newRoot"),
    oldNextIndex: wireInteger(row["oldNextIndex"], "oldNextIndex", 1n, QUAD_CAPACITY),
    newNextIndex: wireInteger(row["newNextIndex"], "newNextIndex", 1n, QUAD_CAPACITY),
    chainStart: checkedHash(row["chainStart"], "chainStart"),
    chainEnd: checkedHash(row["chainEnd"], "chainEnd"),
    writes: array(row["writes"], "writes", recordWrite),
  };
}

function recordWrite(entry: unknown, path: string): RingRecordWriteView {
  const item = object(entry, path, [
    "member",
    "mask",
    "value",
    "mintPolicy",
    "exists",
    "oldNext",
    "oldLists",
    "oldMintPolicy",
    "slot",
    "low",
    "lowPath",
  ]);
  return {
    member: checkedHash(item["member"], `${path}.member`),
    mask: u64(item["mask"], `${path}.mask`),
    value: u64(item["value"], `${path}.value`),
    mintPolicy: item["mintPolicy"] == null ? null : decodeMintPolicy(item["mintPolicy"]),
    exists: boolean(item["exists"], `${path}.exists`),
    oldNext: checkedHash(item["oldNext"], `${path}.oldNext`),
    oldLists: u64(item["oldLists"], `${path}.oldLists`),
    oldMintPolicy: decodeMintPolicy(item["oldMintPolicy"]),
    slot: recordPath(item["slot"], `${path}.slot`),
    low: memberRecord(item["low"], `${path}.low`),
    lowPath: recordPath(item["lowPath"], `${path}.lowPath`),
  };
}

function decodeMintPolicy(value: unknown): MintPolicy {
  const row = object(value, "mintPolicy", [
    "threshold",
    "cap",
    "cosignAbove",
    "withdrawalThreshold",
    "limits",
  ]);
  const amount = (from: WireObject, key: string): bigint => u64(from[key], `mintPolicy.${key}`);
  const limits = object(row["limits"], "mintPolicy.limits", ASSET_LIMIT_KEYS);
  return Object.freeze({
    threshold: amount(row, "threshold"),
    cap: amount(row, "cap"),
    cosignAbove: amount(row, "cosignAbove"),
    withdrawalThreshold: amount(row, "withdrawalThreshold"),
    limits: Object.freeze({
      senderCap: amount(limits, "senderCap"),
      senderWindowSlots: amount(limits, "senderWindowSlots"),
      volumeCap: amount(limits, "volumeCap"),
      volumeWindowSlots: amount(limits, "volumeWindowSlots"),
      depositCap: amount(limits, "depositCap"),
      depositWindowSlots: amount(limits, "depositWindowSlots"),
      withdrawalCap: amount(limits, "withdrawalCap"),
      withdrawalWindowSlots: amount(limits, "withdrawalWindowSlots"),
    }),
  });
}
