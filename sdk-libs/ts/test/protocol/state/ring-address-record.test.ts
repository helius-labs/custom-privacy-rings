import { address } from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { readFileSync } from "node:fs";

import emptyVector from "../../../../../test-vectors/registry/empty_root.json" with { type: "json" };
import { initializePoseidon } from "@heliuslabs/spr";
import { encodeBase58, poseidon } from "@heliuslabs/spr/internal";
import type { Bytes32, Bytes64 } from "@heliuslabs/spr/interface";

import {
  MEMBER_REGISTRY_EMPTY_ROOT,
  SENTINEL_RECORD,
  memberRecordHash,
  quadPathRoot,
} from "../../../src/protocol/membership/member-opening.js";
import { QUAD_HEIGHT, chainLink } from "../../../src/protocol/membership/member-registry.js";
import { TRACKING_QUAD_HEIGHT } from "../../../src/protocol/membership/tracking.js";
import {
  TRACKING_EMPTY_ROOT,
  TRACKING_SENTINEL,
  trackingEntryHash,
  trackingRecordHash,
} from "../../../src/protocol/membership/tracking.js";
import {
  encodeEnrollmentRecord,
  encodeEnrollmentWrite,
  signedWrite,
  enrollmentWriteEntry,
} from "../../../src/protocol/ring-rpc/enrollment-write.js";
import { recordTree } from "../../helpers/member-registry.js";

function field(value: string): Bytes32 {
  const decoded = Uint8Array.from(Buffer.from(value, "hex"));
  if (decoded.length !== 32) throw new RangeError("expected 32 bytes");
  return decoded as Bytes32;
}

function hex(value: Readonly<ArrayLike<number>> | undefined): string {
  return Buffer.from(Uint8Array.from(value ?? [])).toString("hex");
}

/** The tracking fields the core generator writes beside the member registry's. */
const empty = emptyVector as typeof emptyVector &
  Readonly<{ tracking_sentinel: Readonly<{ leaf_hash: string }>; tracking_empty_root: string }>;

const enroll = JSON.parse(
  readFileSync(
    new URL("../../../../../test-vectors/registry/tracking_enroll.json", import.meta.url),
    "utf8",
  ),
) as Readonly<
  Record<
    | "registry"
    | "owner"
    | "enroll_write"
    | "enroll_entry_hash"
    | "enroll_message"
    | "rotate_write"
    | "rotate_entry_hash"
    | "rotate_message"
    | "chain_start"
    | "chain_end",
    string
  >
>;

const REGISTRY = address(encodeBase58(field(enroll.registry)));
const OWNER = address(encodeBase58(field(enroll.owner)));
const bytesOf = (written: string, from: number, length: number) =>
  Uint8Array.from(Buffer.from(written.slice(2 * from, 2 * (from + length)), "hex"));
/** A write is owner || kind || body. */
const data = (from: number, length: number) => bytesOf(enroll.enroll_write, 33 + from, length);
const VIEWING_U = data(64, 32) as Bytes32;
const SEAL = {
  nullifierPk: data(0, 32) as Bytes32,
  listTag: data(32, 32) as Bytes32,
  ephU: data(96, 32) as Bytes32,
  ciphertext: data(128, 64) as Bytes64,
};
const WRITE = {
  owner: OWNER,
  change: {
    kind: "enroll",
    record: { ...SEAL, viewingU: VIEWING_U },
    proof: data(192, 192),
  },
} as const;
const ROTATE = {
  owner: OWNER,
  change: {
    kind: "rotate",
    viewingU: bytesOf(enroll.rotate_write, 33, 32) as Bytes32,
    priorViewingHash: bytesOf(enroll.rotate_write, 65, 32) as Bytes32,
  },
} as const;
/** Hashed after `initializePoseidon`. */
const entry = () => enrollmentWriteEntry(WRITE);

beforeAll(initializePoseidon);

describe("empty_root.json", () => {
  it("folds the sentinel over the zero subtrees to the registry's first root", () => {
    expect(hex(memberRecordHash(SENTINEL_RECORD))).toBe(empty.sentinel.leaf_hash);
    expect(empty.zero_subtrees).toHaveLength(TRACKING_QUAD_HEIGHT + 1);
    empty.zero_subtrees.slice(1).forEach((node, level) => {
      const below = field(empty.zero_subtrees[level] ?? "");
      expect(hex(poseidon([below, below, below, below]))).toBe(node);
    });
    const siblings = empty.zero_subtrees
      .slice(0, QUAD_HEIGHT)
      .map((node) => [field(node), field(node), field(node)]);
    const root = quadPathRoot({ index: 0, siblings }, memberRecordHash(SENTINEL_RECORD));
    expect(hex(root)).toBe(empty.empty_root);
    expect(hex(MEMBER_REGISTRY_EMPTY_ROOT)).toBe(empty.empty_root);
    expect(hex(recordTree().root)).toBe(empty.empty_root);
  });

  it("folds the tracking sentinel over the same subtrees to the tracking registry's first root", () => {
    const sentinel = trackingRecordHash(TRACKING_SENTINEL);
    expect(hex(sentinel)).toBe(empty.tracking_sentinel.leaf_hash);
    const siblings = empty.zero_subtrees
      .slice(0, TRACKING_QUAD_HEIGHT)
      .map((node) => [field(node), field(node), field(node)]);
    const root = siblings.reduce<Bytes32>(
      (node, level) => poseidon([node, ...level]) as Bytes32,
      sentinel,
    );
    expect(hex(root)).toBe(empty.tracking_empty_root);
    expect(hex(TRACKING_EMPTY_ROOT)).toBe(empty.tracking_empty_root);
  });
});

describe("tracking_enroll.json", () => {
  it("encodes each write byte for byte", () => {
    expect(hex(encodeEnrollmentWrite(WRITE))).toBe(enroll.enroll_write);
    expect(hex(encodeEnrollmentRecord(WRITE.change.record))).toBe(
      enroll.enroll_write.slice(2 * 33, 2 * 225),
    );
    expect(hex(encodeEnrollmentWrite(ROTATE))).toBe(enroll.rotate_write);
  });

  it("signs each entry hash under the registry", () => {
    expect(hex(signedWrite(WRITE, REGISTRY).message)).toBe(enroll.enroll_message);
    expect(hex(signedWrite(ROTATE, REGISTRY).message)).toBe(enroll.rotate_message);
  });

  it("hashes both entries and chains them from the apply's start", () => {
    const first = trackingEntryHash(entry());
    const rotation = trackingEntryHash(enrollmentWriteEntry(ROTATE));
    expect(hex(first)).toBe(enroll.enroll_entry_hash);
    expect(hex(rotation)).toBe(enroll.rotate_entry_hash);
    expect(hex(chainLink(chainLink(field(enroll.chain_start), first), rotation))).toBe(
      enroll.chain_end,
    );
    const moved = { ...entry(), listTag: field(enroll.rotate_entry_hash) };
    expect(hex(trackingEntryHash(moved))).not.toBe(enroll.enroll_entry_hash);
  });
});
