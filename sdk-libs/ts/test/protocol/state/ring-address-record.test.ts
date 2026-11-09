import { address } from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { readFileSync } from "node:fs";

import emptyVector from "../../../../../test-vectors/registry/empty_root.json" with { type: "json" };
import { initializePoseidon } from "@heliuslabs/spr";
import { encodeBase58, poseidon, solanaOwnerIdentity } from "@heliuslabs/spr/internal";
import type { Bytes32, Bytes64 } from "@heliuslabs/spr/interface";

import { enrollInstruction } from "../../../src/operations/management/enrollment.js";
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
  trackingEntryOf,
  trackingRecordHash,
} from "../../../src/protocol/membership/tracking.js";
import { enrollmentQueuePda, STORAGE_PROGRAM_ID } from "../../../src/protocol/pda/index.js";
import { recordTree } from "../../helpers/member-registry.js";
import { trackingClient } from "../../helpers/tracking-client.js";

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
    | "queue"
    | "member"
    | "ct_hash"
    | "viewing_hash"
    | "enroll_data"
    | "entry_hash"
    | "chain_start"
    | "chain_end",
    string
  > & { queue_bump: number; proven: boolean }
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
/** Hashed after `initializePoseidon`. */
const entry = () =>
  trackingEntryOf({
    member: field(enroll.member),
    nullifierPk: SEAL.nullifierPk,
    listTag: SEAL.listTag,
    viewingU: VIEWING_U,
    ciphertext: SEAL.ciphertext,
    proven: enroll.proven,
  });

const UNENROLLED = trackingClient(REGISTRY, { applied: [], pending: [] });

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
  it("builds the enrollment byte for byte with the program's account order", async () => {
    const instruction = await enrollInstruction(
      { owner: OWNER, trackingRegistry: REGISTRY, seal: SEAL, viewingU: VIEWING_U },
      UNENROLLED,
    );
    expect(hex(instruction.data)).toBe(`06${enroll.enroll_data}`);
    expect(instruction.programAddress).toBe(STORAGE_PROGRAM_ID);
    expect(instruction.accounts?.map(({ address: account }) => account)).toEqual([
      OWNER,
      REGISTRY,
      encodeBase58(field(enroll.queue)),
    ]);
  });

  it("appends a first enrollment's proof and refuses another length", async () => {
    const proof = new Uint8Array(192).fill(5);
    const input = { owner: OWNER, trackingRegistry: REGISTRY, seal: SEAL, viewingU: VIEWING_U };
    const first = await enrollInstruction({ ...input, proof }, UNENROLLED);
    expect(hex(first.data)).toBe(`06${enroll.enroll_data}${hex(proof)}`);
    await expect(
      enrollInstruction({ ...input, proof: proof.subarray(1) }, UNENROLLED),
    ).rejects.toMatchObject({
      code: "RING_PROOF_LENGTH",
    });
  });

  it("derives the queue PDA from the registry", async () => {
    const [queue, bump] = await enrollmentQueuePda(REGISTRY);
    expect(queue).toBe(encodeBase58(field(enroll.queue)));
    expect(bump).toBe(enroll.queue_bump);
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
