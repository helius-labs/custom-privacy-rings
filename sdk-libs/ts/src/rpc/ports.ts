import type { X25519PublicKey } from "@heliuslabs/spr/keypair";
import type { Bytes32, Bytes64, RequestContext } from "@heliuslabs/spr";
import type { Transaction } from "@heliuslabs/spr/interface";
import type {
  BlockhashProvider,
  ChainReader,
  MergeAssembler,
  ProvenRingTransact,
  Prover,
  TreeContext,
} from "@heliuslabs/spr/client";
import type { Address, Signature } from "@solana/kit";

import type {
  RingMemberOpening,
  RingServedRecord,
} from "../protocol/membership/member-registry.js";
import type { RingTransactInstructionData } from "../protocol/proof/circuit.js";
import type {
  TrackingOpenings,
  TrackingOpeningsRequest,
  TrackingRecordPoolRequest,
} from "../protocol/indexer/tracking-codec.js";
import type { ServedTrackingPool } from "../protocol/membership/tracking.js";
import type { RingProver } from "../protocol/proof/ring-proof-requests.js";

export type { RingMemberOpening, RingProver, RingServedRecord };

/** The nullifier key an owner enrolled, sealed to the auditor. */
export interface RingSealedKey {
  /** The signer whose owner identity is the tracking row's member. */
  readonly owner: Address;
  readonly nullifierPk: Bytes32;
  readonly ephemeralPublicKey: X25519PublicKey;
  readonly ciphertext: Bytes64;
}

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
  readonly context: Readonly<{ slot: bigint; blockTime: bigint }>;
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
  readonly context: Readonly<{ slot: bigint; blockTime: bigint }>;
  readonly root: StorageRoot;
  readonly records: readonly RingServedRecord[];
  /** `undefined` on the last page. */
  readonly nextLeafIndex: number | undefined;
}

/** Records under one root, unverified until `verifyRecordPool` checks them against it. */
export interface StorageRecordPool {
  readonly context: Readonly<{ slot: bigint; blockTime: bigint }>;
  readonly root: StorageRoot;
  /** Distinct leaves in first use order, each path cut at the canopy. */
  readonly records: readonly RingServedRecord[];
  readonly canopy: readonly Bytes32[];
  /** `subjects[i]` indexes the record holding or bracketing member `i`. */
  readonly subjects: readonly number[];
}

export interface RingRecordReader {
  getStorageMemberOpenings(
    request: StorageOpeningsRequest,
    context?: RequestContext,
  ): Promise<StorageMemberOpenings>;
  getStorageMemberRecords(
    request: StoragePageRequest,
    context?: RequestContext,
  ): Promise<StorageMemberPage>;
  getStorageRecordPool(
    request: StorageRecordPoolRequest,
    context?: RequestContext,
  ): Promise<StorageRecordPool>;
}

export interface TrackingReader {
  getTrackingOpenings(
    request: TrackingOpeningsRequest,
    context?: RequestContext,
  ): Promise<TrackingOpenings>;
  getTrackingRecordPool(
    request: TrackingRecordPoolRequest,
    context?: RequestContext,
  ): Promise<ServedTrackingPool>;
}

export type RingSubmissionStatus =
  | Readonly<{ kind: "unknown" }>
  | Readonly<{ kind: "confirmed"; slot: bigint }>
  | Readonly<{ kind: "failed"; instructionIndex?: number; customCode?: number }>
  | Readonly<{ kind: "expired" }>;

/** Identifies a broadcast awaiting confirmation or expiry. */
export interface RingSubmissionPending {
  readonly signature: Signature;
  readonly lastValidBlockHeight: bigint;
}

/** Separates signing and broadcast from submission status resolution. */
export interface RingSubmissionTransport {
  sign(transaction: Transaction, context?: RequestContext): Promise<Transaction>;
  /** A refusal before broadcast returns `failed`, a throw leaves the signature pending. */
  send(
    transaction: Transaction,
    context?: RequestContext,
  ): Promise<RingSubmissionStatus | undefined>;
  status(pending: RingSubmissionPending, context?: RequestContext): Promise<RingSubmissionStatus>;
}

export type RingMergeClient = TreeContext &
  BlockhashProvider &
  Pick<ChainReader, "getAccount"> &
  Pick<MergeAssembler, "proveMerge"> &
  RingProver;

/** A nullifier key encrypted to the ring auditor. */
export interface RingSealedNullifierKey {
  readonly ephemeralPublicKey: X25519PublicKey;
  readonly ciphertext: Bytes64;
}

export type ProvenAnonymousRingTransact = Omit<ProvenRingTransact, "data"> &
  Readonly<{ data: RingTransactInstructionData }>;

/** A confidential ring's in-proof owner also proves on it. */
export type AnonymousRingTransactProver = Pick<Prover, "proveAnonymousRingTransact">;
