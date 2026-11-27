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
import type {
  StorageMemberOpenings,
  StorageMemberPage,
  StorageOpeningsRequest,
  StoragePageRequest,
  StorageRecordPool,
  StorageRecordPoolRequest,
  StorageRoot,
} from "../protocol/indexer/indexer-types.js";
import type { ServedTrackingPool } from "../protocol/membership/tracking.js";
import type { RingProver } from "../protocol/proof/ring-proof-requests.js";

export type {
  RingMemberOpening,
  RingProver,
  RingServedRecord,
  StorageMemberOpenings,
  StorageMemberPage,
  StorageOpeningsRequest,
  StoragePageRequest,
  StorageRecordPool,
  StorageRecordPoolRequest,
  StorageRoot,
};

/** The nullifier key an owner enrolled, sealed to the auditor. */
export interface RingSealedKey {
  /** The signer whose owner identity is the tracking row's member. */
  readonly owner: Address;
  readonly nullifierPk: Bytes32;
  readonly ephemeralPublicKey: X25519PublicKey;
  readonly ciphertext: Bytes64;
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
