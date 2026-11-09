import { checkedBytes, equalBytes, poseidon, ZERO_32 } from "@heliuslabs/spr/internal";
import type { Bytes32 } from "@heliuslabs/spr/interface";

import type { MintPolicy } from "./mint-policy.js";

/** Rust `QUAD_ARITY` and `QUAD_HEIGHT`, the 4-ary shape of the member registry and the tracking tree. */
export const QUAD_ARITY = 4;
export const QUAD_HEIGHT = 16;
/** Rust `QUAD_CAPACITY`, `4^16` leaves, a u64 next index holds it. */
export const QUAD_CAPACITY = 1n << BigInt(2 * QUAD_HEIGHT);
/** Rust `ROOT_HISTORY`, the roots a registry keeps. */
export const ROOT_HISTORY = 32;
/** Rust `APPLY_BATCH`, the writes the larger apply key proves. */
export const APPLY_BATCH = 16;

export interface MemberRecord {
  readonly member: Bytes32;
  readonly next: Bytes32;
  /** Bit `id - 1` marks list `id`. */
  readonly lists: bigint;
  readonly mintPolicy: MintPolicy;
}

/** A leaf index and three siblings per level in ascending child order, leaf level first. */
export interface QuadPath {
  readonly index: number;
  readonly siblings: readonly (readonly Bytes32[])[];
}

/** The least `levels` with `4^levels >= openings`, at most `QUAD_HEIGHT`. */
export function canopyLevels(openings: number): number {
  let levels = 0;
  while (levels < QUAD_HEIGHT && QUAD_ARITY ** levels < openings) levels += 1;
  return levels;
}

export type TreeRefusal = (reason: string) => never;

/** The root `leaf` folds to along `path`, Rust `QuadPath::root`. */
export function foldQuadPath(path: QuadPath, leaf: Bytes32, refuse: TreeRefusal): Bytes32 {
  if (!Number.isSafeInteger(path.index) || path.index < 0 || BigInt(path.index) >= QUAD_CAPACITY)
    refuse("IndexOutOfRange");
  if (path.siblings.length !== QUAD_HEIGHT) refuse("PathHeight");
  let node = leaf;
  path.siblings.forEach((siblings, level) => {
    if (siblings.length !== QUAD_ARITY - 1) refuse("PathArity");
    const digit = Math.floor(path.index / QUAD_ARITY ** level) % QUAD_ARITY;
    const children = siblings.map((sibling) => checkedBytes<Bytes32>(sibling, 32, "sibling"));
    children.splice(digit, 0, node);
    node = poseidon(children) as Bytes32;
  });
  return node;
}

/** Each pool path completed with the canopy nodes above its cut, a canopy off `root` refuses, Rust `SharedProof::verify`. */
export function coveredPaths(
  input: Readonly<{
    paths: readonly QuadPath[];
    canopy: readonly Bytes32[];
    openings: number;
    root: Bytes32;
  }>,
  refuse: TreeRefusal,
): readonly QuadPath[] {
  const levels = canopyLevels(input.openings);
  if (input.canopy.length !== QUAD_ARITY ** levels) refuse("PoolShape");
  const canopy = [input.canopy.map((node) => checkedBytes<Bytes32>(node, 32, "canopy"))];
  for (let level = 0; level < levels; level += 1) {
    const below = canopy[level] ?? [];
    canopy.push(
      Array.from(
        { length: below.length / QUAD_ARITY },
        (_, parent) =>
          poseidon(below.slice(parent * QUAD_ARITY, (parent + 1) * QUAD_ARITY)) as Bytes32,
      ),
    );
  }
  if (!equalBytes(canopy[levels]?.[0] ?? ZERO_32, input.root)) refuse("RootMismatch");
  const height = QUAD_HEIGHT - levels;
  return input.paths.map((path) => {
    if (path.siblings.length !== height) refuse("PathHeight");
    return Object.freeze({
      index: path.index,
      siblings: Object.freeze([
        ...path.siblings,
        ...canopy.slice(0, levels).map((nodes, level) => {
          const node = Math.floor(path.index / QUAD_ARITY ** (height + level));
          const first = node - (node % QUAD_ARITY);
          return nodes
            .slice(first, first + QUAD_ARITY)
            .filter((_, child) => first + child !== node);
        }),
      ]),
    });
  });
}

/** `Poseidon(previous, entry)`, Rust `chain_link`, one step of a records or enrollment chain. */
export function chainLink(previous: Bytes32, entry: Bytes32): Bytes32 {
  return poseidon([
    checkedBytes<Bytes32>(previous, 32, "chain"),
    checkedBytes<Bytes32>(entry, 32, "entry"),
  ]) as Bytes32;
}

/** Go `PooledSubject`, `recordIndex` the record holding or bracketing `member`. */
export interface PoolSubject {
  readonly member: Bytes32;
  readonly recordIndex: number;
}

/** Go `PooledRecord` rows, their `SharedRegistryProof` and the `PooledSubject` rows they open. */
export interface RecordPool {
  readonly records: readonly MemberRecord[];
  readonly canopy: readonly Bytes32[];
  /** `paths[i]` opens `records[i]` up to the canopy, leaf level first. */
  readonly paths: readonly QuadPath[];
  readonly subjects: readonly PoolSubject[];
}

/** A member under one root, its record or the low record bracketing it. */
export type RecordOpening =
  | Readonly<{ kind: "present"; record: MemberRecord; path: QuadPath }>
  | Readonly<{ kind: "absent"; low: MemberRecord; path: QuadPath }>;

/** `lists' = (lists & !mask) | value`. */
export interface RecordWrite {
  readonly member: Bytes32;
  readonly mask: bigint;
  readonly value: bigint;
  readonly mintPolicy?: MintPolicy;
}

/** One write's apply proof input, Rust `RecordWriteProofInput`. */
export interface RecordWriteProofInput {
  readonly entry: RecordWrite;
  readonly exists: boolean;
  /** An absent member reads its low record's `next` and empty lists. */
  readonly oldNext: Bytes32;
  readonly oldLists: bigint;
  readonly oldMintPolicy: MintPolicy;
  /** The record's leaf, for an absent member the leaf at `next_index`, the low leaf for a no-op on a full tree. */
  readonly slot: QuadPath;
  /** An absent member's low record and its path before the relink, empty for a present member. */
  readonly low: MemberRecord;
  readonly lowPath: QuadPath;
}

/** One application's proof inputs, Rust `ApplyProofInputs`. */
export interface ApplyProofInputs {
  readonly oldRoot: Bytes32;
  readonly newRoot: Bytes32;
  readonly oldNextIndex: bigint;
  readonly newNextIndex: bigint;
  /** `chain_start` of the registry and revision. */
  readonly chainStart: Bytes32;
  /** The chain past the last write. */
  readonly chainEnd: Bytes32;
  /** Instruction order. */
  readonly writes: readonly RecordWriteProofInput[];
}
