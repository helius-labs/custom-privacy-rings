import type { MintPolicy } from "../membership/mint-policy.js";
import type {
  MemberRecord,
  QuadPath,
  RingMemberOpening,
  RingServedRecord,
} from "../membership/member-registry.js";
import type { Address, Bytes32 } from "@heliuslabs/spr";
import type { indexerTypes } from "@heliuslabs/spr/internal";

type Hash = indexerTypes.Hash;
type IndexerContext = indexerTypes.IndexerContext;

/** A root a `StorageRegistry` history holds, named by the revision the ring program reads. */
export interface StorageRoot {
  readonly root: Bytes32;
  readonly revision: bigint;
  readonly nextIndex: bigint;
}

/** At most `MAX_RING_MEMBER_BATCH` members of one `StorageRegistry`, all under `expectedRoot`. */
export interface StorageOpeningsRequest {
  readonly registry: Address;
  readonly members: readonly Bytes32[];
  readonly expectedRoot: Bytes32;
}

export interface StorageRecordPoolRequest extends StorageOpeningsRequest {
  readonly openings: number;
}

export interface StorageMemberOpenings {
  readonly context: IndexerContext;
  readonly root: StorageRoot;
  /** In request order. */
  readonly openings: readonly RingMemberOpening[];
}

/** Leaves of one `StorageRegistry` from `startIndex`, all under `expectedRoot`. */
export interface StoragePageRequest {
  readonly registry: Address;
  readonly expectedRoot: Bytes32;
  readonly startIndex: number;
  readonly limit: number;
}

/** Leaves from `startIndex` at one root, each with its path. */
export interface StorageMemberPage {
  readonly context: IndexerContext;
  readonly root: StorageRoot;
  readonly records: readonly RingServedRecord[];
  /** `undefined` on the last page. */
  readonly nextLeafIndex: number | undefined;
}

/** Records under one root, unverified until `verifyRecordPool` checks them against it. */
export interface StorageRecordPool {
  readonly context: IndexerContext;
  readonly root: StorageRoot;
  /** Distinct leaves in first use order, each path cut at the canopy. */
  readonly records: readonly RingServedRecord[];
  readonly canopy: readonly Bytes32[];
  /** `subjects[i]` indexes the record holding or bracketing member `i`. */
  readonly subjects: readonly number[];
}

export interface RingListWriteView {
  readonly member: Hash;
  readonly mask: bigint;
  readonly value: bigint;
  readonly mintPolicy: MintPolicy | null;
}

/** Paths are under the root the preceding writes reached. */
export interface RingRecordWriteView {
  readonly member: Hash;
  readonly mask: bigint;
  readonly value: bigint;
  readonly exists: boolean;
  readonly oldNext: Hash;
  readonly oldLists: bigint;
  readonly oldMintPolicy: MintPolicy;
  readonly slot: QuadPath;
  readonly low: MemberRecord;
  readonly lowPath: QuadPath;
  readonly mintPolicy: MintPolicy | null;
}

/** Rust `StorageWriteView`. */
export type StorageWriteView = RingListWriteView;

/** Rust `GetStorageApplyProofInputsRequest`. */
export interface StorageApplyProofInputsRequest {
  readonly registry: Address;
  readonly writes: readonly StorageWriteView[];
}

export interface StorageApplyProofInputsResponse {
  readonly context: IndexerContext;
  readonly revision: bigint;
  readonly oldRoot: Hash;
  readonly newRoot: Hash;
  readonly oldNextIndex: bigint;
  readonly newNextIndex: bigint;
  readonly chainStart: Hash;
  readonly chainEnd: Hash;
  /** In request order. */
  readonly writes: readonly RingRecordWriteView[];
}
