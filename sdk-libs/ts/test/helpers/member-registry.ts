import { EMPTY_MINT_POLICY, type MintPolicy } from "../../src/protocol/membership/mint-policy.js";
import type { Bytes32 } from "@heliuslabs/spr/interface";
import { ZERO_32, bytesToBigInt, hashBytes, poseidon } from "@heliuslabs/spr/internal";
import { X25519Keypair, type ShieldedAddress } from "@heliuslabs/spr/keypair";
import {
  QUAD_ARITY,
  QUAD_HEIGHT,
  canopyLevels,
  type MemberRecord,
  type QuadPath,
} from "../../src/protocol/membership/member-registry.js";
import { FIELD_MAX, memberRecordHash } from "../../src/protocol/membership/member-opening.js";
import type { TrackingSeal } from "../../src/protocol/indexer/tracking-codec.js";
import {
  TRACKING_SENTINEL,
  TRACKING_QUAD_HEIGHT,
  trackingRecordHash,
  viewingKeyHash,
  type TrackingFields,
  type TrackingRecord,
} from "../../src/protocol/membership/tracking.js";

/** A leaf the fixture tracking tree holds, with the seal the indexer serves beside it. */
export type FixtureTrackingEntry = TrackingFields & Readonly<{ seal?: TrackingSeal }>;

const FIXTURE_EPH_PK = X25519Keypair.fromBytes(new Uint8Array(32).fill(0x22)).publicKey().toBytes();

/** `owner`'s applied enrollment, its leaf binding the served viewing key and ciphertext. */
export function enrolledEntry(
  owner: ShieldedAddress,
  listTag: Bytes32 = ZERO_32,
): FixtureTrackingEntry {
  const ciphertext = new Uint8Array(64).fill(0x33);
  const viewingU = owner.viewingPublicKey.u();
  return {
    member: owner.signingPublicKey.ownerProofInputHash(),
    nullifierPk: owner.nullifierPublicKey,
    ctHash: hashBytes(ciphertext) as Bytes32,
    listTag,
    viewingHash: viewingKeyHash(viewingU, 0n),
    seal: { owner: owner.solanaAddress(), viewingU, seq: 0n, ephPk: FIXTURE_EPH_PK, ciphertext },
  };
}

/** One record the fixture holds. */
export interface FixtureRecord {
  readonly member: Bytes32;
  readonly lists: bigint;
  readonly mintPolicy?: MintPolicy;
}

/** A sparse 4-ary indexed tree, leaf 0 the sentinel and the members in key order after it. */
function indexedTree<Row extends Readonly<{ member: Bytes32; next: Bytes32 }>>(
  rows: readonly Row[],
  leafHash: (row: Row) => Bytes32,
  height: number,
) {
  const zeros = [ZERO_32];
  for (let level = 0; level < height; level += 1) {
    const below = zeros[level] ?? ZERO_32;
    zeros.push(poseidon([below, below, below, below]));
  }
  const layers: Map<number, Bytes32>[] = [
    new Map(rows.map((row, index) => [index, leafHash(row)])),
  ];
  for (let level = 0; level < height; level += 1) {
    const below = layers[level] ?? new Map<number, Bytes32>();
    const above = new Map<number, Bytes32>();
    for (const index of below.keys()) {
      const parent = index >> 2;
      if (above.has(parent)) continue;
      const children = Array.from(
        { length: QUAD_ARITY },
        (_, child) => below.get(parent * QUAD_ARITY + child) ?? zeros[level] ?? ZERO_32,
      );
      above.set(parent, poseidon(children));
    }
    layers.push(above);
  }
  const root = layers[height]?.get(0) ?? ZERO_32;
  const path = (index: number): QuadPath => ({
    index,
    siblings: Array.from({ length: height }, (_, level) => {
      const node = index >> (2 * level);
      const parent = node >> 2;
      return Array.from({ length: QUAD_ARITY }, (_, child) => parent * QUAD_ARITY + child)
        .filter((sibling) => sibling !== node)
        .map((sibling) => layers[level]?.get(sibling) ?? zeros[level] ?? ZERO_32);
    }),
  });
  const opening = (member: Bytes32) => {
    const index = rows.findIndex(
      (row, at) => at > 0 && bytesToBigInt(row.member) === bytesToBigInt(member),
    );
    const record = rows[index];
    if (index > 0 && record !== undefined)
      return { kind: "present" as const, record, path: path(index) };
    const value = bytesToBigInt(member);
    const lowIndex = rows.findIndex(
      (row) => bytesToBigInt(row.member) < value && value < bytesToBigInt(row.next),
    );
    const low = rows[lowIndex];
    if (low === undefined) throw new Error("member outside the fixture registry");
    return { kind: "absent" as const, low, path: path(lowIndex) };
  };
  /** The record pool response, distinct leaves in first use order. */
  const pool = (members: readonly Bytes32[], openings: number) => {
    const levels = canopyLevels(openings, height);
    const cut = height - levels;
    const records: { record: Row; path: QuadPath }[] = [];
    const subjects = members.map((member) => {
      const { index, siblings } = opening(member).path;
      const at = records.findIndex((served) => served.path.index === index);
      if (at >= 0) return at;
      const record = rows[index];
      if (record === undefined) throw new Error("leaf outside the fixture registry");
      records.push({ record, path: { index, siblings: siblings.slice(0, cut) } });
      return records.length - 1;
    });
    const canopy = Array.from(
      { length: QUAD_ARITY ** levels },
      (_, node) => layers[cut]?.get(node) ?? zeros[cut] ?? ZERO_32,
    );
    return { records, canopy, subjects };
  };
  return { root, nextIndex: BigInt(rows.length), rows, path, opening, pool };
}

function ascending<Entry extends Readonly<{ member: Bytes32 }>>(
  entries: readonly Entry[],
): readonly Entry[] {
  return [...entries].sort((a, b) => (bytesToBigInt(a.member) < bytesToBigInt(b.member) ? -1 : 1));
}

export function recordTree(records: readonly FixtureRecord[] = []) {
  const sorted = ascending(records);
  const rows: MemberRecord[] = [
    {
      mintPolicy: EMPTY_MINT_POLICY,
      member: ZERO_32,
      lists: 0n,
      next: sorted[0]?.member ?? FIELD_MAX,
    },
    ...sorted.map((record, index) => ({
      member: record.member,
      lists: record.lists,
      mintPolicy: record.mintPolicy ?? EMPTY_MINT_POLICY,
      next: sorted[index + 1]?.member ?? FIELD_MAX,
    })),
  ];
  return indexedTree(rows, memberRecordHash, QUAD_HEIGHT);
}

/** Present openings carry their entry's seal, as the indexer serves them. */
export function trackingTree(entries: readonly FixtureTrackingEntry[] = []) {
  const sorted = ascending(entries);
  const rows: TrackingRecord[] = [
    { ...TRACKING_SENTINEL, next: sorted[0]?.member ?? FIELD_MAX },
    ...sorted.map((entry, index) => ({
      member: entry.member,
      nullifierPk: entry.nullifierPk,
      ctHash: entry.ctHash,
      listTag: entry.listTag,
      viewingHash: entry.viewingHash,
      next: sorted[index + 1]?.member ?? FIELD_MAX,
    })),
  ];
  const tree = indexedTree(rows, trackingRecordHash, TRACKING_QUAD_HEIGHT);
  const seals = new Map(
    entries.flatMap(({ member, seal }) =>
      seal === undefined ? [] : [[bytesToBigInt(member), seal] as const],
    ),
  );
  const opening = (member: Bytes32) => {
    const read = tree.opening(member);
    const seal = read.kind === "present" ? seals.get(bytesToBigInt(member)) : undefined;
    return seal === undefined ? read : { ...read, seal };
  };
  const pool = (members: readonly Bytes32[], openings: number) => {
    const served = tree.pool(members, openings);
    return {
      ...served,
      records: served.records.map((entry) => {
        const seq = seals.get(bytesToBigInt(entry.record.member))?.seq;
        return seq === undefined ? entry : { ...entry, seq };
      }),
    };
  };
  return { ...tree, opening, pool };
}
