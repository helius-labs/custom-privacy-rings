import { address } from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { readFileSync } from "node:fs";

import emptyVector from "../../../../../test-vectors/registry/empty_root.json" with { type: "json" };
import { initializePoseidon } from "@heliuslabs/spr";
import { encodeBase58, poseidon, solanaOwnerIdentity } from "@heliuslabs/spr/internal";
import type { Bytes32, Bytes64 } from "@heliuslabs/spr/interface";

import {
  MEMBER_REGISTRY_EMPTY_ROOT,
  SENTINEL_RECORD,
  memberRecordHash,
  quadPathRoot,
} from "../../../src/protocol/membership/member-opening.js";
import { QUAD_HEIGHT, chainLink } from "../../../src/protocol/membership/member-registry.js";
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
} from "../../../src/protocol/ring-rpc/relay-enroll-inputs.js";
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
    | "member"
    | "ct_hash"
    | "viewing_hash"
    | "enroll_data"
    | "entry_hash"
    | "chain_start"
    | "chain_end",
    string
  >
>;

const REGISTRY = address(encodeBase58(field(enroll.registry)));
const OWNER = address(encodeBase58(field(enroll.owner)));
const data = (from: number, length: number) =>
  Uint8Array.from(Buffer.from(enroll.enroll_data.slice(2 * from, 2 * (from + length)), "hex"));
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
    proof: new Uint8Array(192).fill(5),
  },
} as const;
/** Hashed after `initializePoseidon`. */
const entry = () => enrollmentWriteEntry(WRITE);

beforeAll(initializePoseidon);

describe("empty_root.json", () => {
  it("folds the sentinel over the zero subtrees to the registry's first root", () => {
    expect(hex(memberRecordHash(SENTINEL_RECORD))).toBe(empty.sentinel.leaf_hash);
    expect(empty.zero_subtrees).toHaveLength(QUAD_HEIGHT + 1);
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
      .slice(0, QUAD_HEIGHT)
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
  it("encodes the record byte for byte inside the Enroll write", () => {
    expect(hex(encodeEnrollmentRecord(WRITE.change.record))).toBe(enroll.enroll_data);
    expect(hex(encodeEnrollmentWrite(WRITE).subarray(33, 225))).toBe(enroll.enroll_data);
  });

  it("signs the entry hash under the registry", () => {
    const message = signedWrite(WRITE, REGISTRY).message;
    expect(hex(message.subarray(9, 41))).toBe(enroll.registry);
    expect(hex(message.subarray(73))).toBe(enroll.entry_hash);
  });

  it("derives the owner's member from its address", () => {
    expect(hex(solanaOwnerIdentity(field(enroll.owner)))).toBe(enroll.member);
  });

  it("hashes the entry from the instruction data and chains it from the apply's start", () => {
    const ENTRY = entry();
    expect(hex(ENTRY.ctHash)).toBe(enroll.ct_hash);
    expect(hex(ENTRY.viewingHash)).toBe(enroll.viewing_hash);
    expect(hex(trackingEntryHash(ENTRY))).toBe(enroll.entry_hash);
    expect(hex(chainLink(field(enroll.chain_start), trackingEntryHash(ENTRY)))).toBe(
      enroll.chain_end,
    );
    const moved = { ...ENTRY, listTag: field(enroll.member) };
    expect(hex(trackingEntryHash(moved))).not.toBe(enroll.entry_hash);
  });
});
