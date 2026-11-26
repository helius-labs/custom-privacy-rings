import type { MessagePartialSigner } from "@solana/kit";
import type { Address, Bytes32, Signature } from "@heliuslabs/spr/interface";
import { ZERO_NULLIFIER_PK, equalBytes } from "@heliuslabs/spr/internal";
import { NullifierKey, type Bytes31 } from "@heliuslabs/spr/keypair";
import { SNAPSHOT_CURSOR_LENGTH } from "../wallet-snapshot.js";
import { type RingReadTarget, targetLines } from "./signed-request.js";
import { type RingReadSigner } from "../../protocol/ring-rpc/read-signer.js";
import { encoder, base64Decoder, invalidRequest } from "./wire.js";
import type {
  DecryptedRingDefaultInput,
  DecryptedRingOutput,
  DecryptedRingWithdrawal,
} from "./audit-reads.js";
import { checkedReadCursor } from "./signed-read.js";

const WALLET_STATUS_DOMAIN = "zolana/ring-rpc-wallet-status";
const SPEND_COVERAGE_DOMAIN = "zolana/ring-rpc-spend-coverage";

export interface WalletStatusAttestationInput {
  readonly target: RingReadTarget;
  readonly wallet: Address;
  readonly timestamp: bigint;
  readonly nonce: Bytes32;
  readonly cursor?: Uint8Array;
}

export function walletStatusAttestation(input: WalletStatusAttestationInput): Uint8Array {
  return walletSnapshotMessage({ ...input, domain: WALLET_STATUS_DOMAIN });
}

export function walletSnapshotReleaseAttestation(
  input: WalletStatusAttestationInput & Readonly<{ cursor: Uint8Array }>,
): Uint8Array {
  return walletSnapshotMessage({
    ...input,
    domain: "zolana/ring-rpc-wallet-snapshot-release",
  });
}

function walletSnapshotMessage(
  input: WalletStatusAttestationInput & Readonly<{ domain: string }>,
): Uint8Array {
  return encoder.encode(
    [
      input.domain,
      ...targetLines(input.target),
      `timestamp: ${input.timestamp}`,
      `nonce: ${base64Decoder.decode(input.nonce)}`,
      `wallet: ${input.wallet}`,
      `cursor: ${input.cursor === undefined ? "" : base64Decoder.decode(input.cursor)}`,
    ].join("\n"),
  );
}

export interface DelegateNotesAttestationInput {
  readonly target: RingReadTarget;
  readonly delegate: Address;
  readonly owner: Address;
  readonly timestamp: bigint;
  readonly nonce: Bytes32;
  readonly cursor?: Uint8Array;
}

export function delegateNotesAttestation(input: DelegateNotesAttestationInput): Uint8Array {
  return delegateSnapshotMessage({ ...input, domain: "zolana/ring-rpc-delegate-notes" });
}

export function delegateSnapshotReleaseAttestation(
  input: DelegateNotesAttestationInput & Readonly<{ cursor: Uint8Array }>,
): Uint8Array {
  return delegateSnapshotMessage({
    ...input,
    domain: "zolana/ring-rpc-delegate-snapshot-release",
  });
}

function delegateSnapshotMessage(
  input: DelegateNotesAttestationInput & Readonly<{ domain: string }>,
): Uint8Array {
  return encoder.encode(
    [
      input.domain,
      ...targetLines(input.target),
      `delegate: ${input.delegate}`,
      `owner: ${input.owner}`,
      `cursor: ${input.cursor === undefined ? "" : base64Decoder.decode(input.cursor)}`,
      `timestamp: ${input.timestamp}`,
      `nonce: ${base64Decoder.decode(input.nonce)}`,
    ].join("\n"),
  );
}

/** Mirrors Rust `SpendCoverageAttestation::message`. */
export function spendCoverageAttestation(
  input: Readonly<{
    target: RingReadTarget;
    authority: Address;
    timestamp: bigint;
    nonce: Bytes32;
    cursor?: Uint8Array;
  }>,
): Uint8Array {
  return encoder.encode(
    [
      SPEND_COVERAGE_DOMAIN,
      ...targetLines(input.target),
      `authority: ${input.authority}`,
      `timestamp: ${input.timestamp}`,
      `nonce: ${base64Decoder.decode(input.nonce)}`,
      `cursor: ${input.cursor === undefined ? "" : base64Decoder.decode(input.cursor)}`,
    ].join("\n"),
  );
}

/** Mirrors Rust `WalletView`, the sender is the anonymous sender or a confidential spender. */
export type RingWalletView =
  | {
      readonly role: "sender";
      /** Nullifiers of the wallet's own spent notes the service resolved. */
      readonly spent: readonly Bytes32[];
      /** The wallet's own default-ring notes. */
      readonly defaultInputs: readonly DecryptedRingDefaultInput[];
      readonly outputs: readonly DecryptedRingOutput[];
      readonly withdrawals: readonly DecryptedRingWithdrawal[];
    }
  | {
      readonly role: "recipient";
      readonly outputs: readonly DecryptedRingOutput[];
      readonly withdrawals: readonly DecryptedRingWithdrawal[];
    };

/** `spent` takes precedence over `exposedIfSpent`, `unknown` never counts as unspent. */
export type RingWalletNoteStatus = "spent" | "unspent" | "unknown" | "exposedIfSpent";

/** Mirrors Rust `WalletNote`, a full opening rebuilds a note whose body was unusable. */
export interface RingWalletNote {
  readonly utxoHash: Bytes32;
  readonly status: RingWalletNoteStatus;
  /** Absent on a deposit whose capsule discloses the owner hash alone, and on a ring note whose owner's registered key the auditor has not read yet. */
  readonly nullifierPk?: Bytes32;
  readonly asset: Bytes32;
  readonly mint?: Address;
  readonly amount: bigint;
  readonly blinding: Bytes32;
  /** The record's proven hash, absent with `ringDataHash` on a record that rebuilt no commitment. */
  readonly dataHash?: Bytes32;
  readonly ringDataHash?: Bytes32;
  readonly treeId: number;
  readonly leafIndex: bigint;
  readonly slot: bigint;
  readonly signature: Signature;
  readonly eventIndex: number;
  readonly slotIndex: number;
}

export interface RingWalletBalance {
  readonly asset: Bytes32;
  readonly mint?: Address;
  readonly amount: bigint;
}

export type RingWalletEnrollment = "notEnrolled" | "applied";

export interface RingWalletStatusQuery {
  readonly target: RingReadTarget;
  readonly signer: RingReadSigner;
  readonly wallet: Address;
  readonly cursor?: Uint8Array;
  readonly timestamp?: bigint;
}

export interface RingDelegateNotesQuery {
  readonly target: RingReadTarget;
  readonly delegate: MessagePartialSigner;
  readonly owner: Address;
  readonly cursor?: Uint8Array;
  readonly timestamp?: bigint;
}

export interface RingDelegateNotesPage {
  /** The registered key's secret, right aligned, absent while the owner holds no registration or its seal is unread. */
  readonly nullifierSecret?: Bytes32;
  /** Any output key pays the owner, a note under a key it never registered opens to no secret here. */
  readonly status: RingWalletStatusPage;
}

/** The key whose public half `note` commits, `undefined` under a key the owner never registered, Rust `GetDelegateNotesResponse::nullifier_key`. */
export function delegateNoteKey(
  page: Pick<RingDelegateNotesPage, "nullifierSecret">,
  note: Pick<RingWalletNote, "nullifierPk">,
): NullifierKey | undefined {
  const committed = note.nullifierPk;
  if (committed === undefined) return undefined;
  if (equalBytes(committed, ZERO_NULLIFIER_PK))
    return NullifierKey.fromSecret(new Uint8Array(31) as Bytes31);
  const secret = page.nullifierSecret;
  if (secret === undefined || secret[0] !== 0) return undefined;
  const key = NullifierKey.fromSecret(secret.slice(1) as Bytes31);
  if (equalBytes(key.publicKey(), committed)) return key;
  key.destroy();
  return undefined;
}

/** Mirrors Rust `GetWalletStatusResponse`, it names nothing about any other transaction. */
export interface RingWalletStatusPage {
  readonly enrollment: RingWalletEnrollment;
  readonly snapshotExpiresAt?: bigint;
  /** Only an unreadable stream holds it back. */
  readonly historyThrough: bigint;
  /** `unspent` notes alone, over every page. */
  readonly balances: readonly RingWalletBalance[];
  readonly notes: readonly RingWalletNote[];
  readonly cursor?: Uint8Array;
}

export interface RingSpendCoverageFrame {
  readonly slot: bigint;
  readonly signature: Signature;
  readonly eventIndex: number;
}

export interface RingSpendCoverageRow {
  readonly nullifier: Bytes32;
  readonly frame: RingSpendCoverageFrame;
}

/** Mirrors Rust `GetSpendCoverageResponse`, the one surface that names an unclassified spend. */
export interface RingSpendCoverage {
  /** Absent inside a paged read. */
  readonly positionSlot?: bigint;
  readonly historyThrough: bigint;
  readonly totals?: RingSpendCoverageTotals;
  /**
   * Transfer nullifiers no opened note owns. A spent note the index never resolved sits among them
   * with addresses, default-ring notes and padding, the proof names no note position.
   */
  readonly unclassified: readonly RingSpendCoverageRow[];
  readonly unreadable: readonly RingSpendCoverageFrame[];
  readonly integrityFaults: readonly RingSpendCoverageRow[];
  readonly cursor?: Uint8Array;
}

export interface RingSpendCoverageTotals {
  readonly observed: bigint;
  /** Merge padding alone. */
  readonly padding: bigint;
  readonly resolved: bigint;
}

/** Mirrors Rust `WalletActivity`. */
export interface RingWalletActivity {
  readonly slot: bigint;
  readonly signature: Signature;
  readonly eventIndex: number;
  readonly view: RingWalletView;
}

export interface RingWalletActivityPage {
  readonly slot: bigint;
  readonly blockTime: bigint;
  readonly items: readonly RingWalletActivity[];
  readonly cursor?: Uint8Array;
}

export function checkedSnapshotCursor(cursor: unknown): Uint8Array {
  const checked = checkedReadCursor(cursor);
  if (checked.length !== SNAPSHOT_CURSOR_LENGTH) throw invalidRequest("cursor");
  return checked;
}
