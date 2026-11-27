import type { Address, Bytes32, Signature } from "@heliuslabs/spr/interface";
import { addressBytes, sha256 } from "@heliuslabs/spr/internal";
import type { X25519PublicKey } from "@heliuslabs/spr/keypair";
import type {
  AuditedEntryInput,
  KnownUtxo,
  OutputOpening,
  OutputPayloadError,
} from "../../operations/audit/audit.js";
import { RingError } from "../../protocol/error.js";
import {
  type RingReadTarget,
  targetLines,
  walletLines,
  checkedReadTarget,
  AttestationRequest,
} from "./signed-request.js";
import {
  type WebAuthnSignature,
  type RingReadSigner,
} from "../../protocol/ring-rpc/read-signer.js";
import { encoder, base64Decoder } from "./wire.js";

const AUDITED_UTXOS_DOMAIN = "zolana/ring-rpc-audited-utxos";
/** Rust `AUDITED_UTXOS_LIMIT`, the UTXO hashes one read names. */
export const RING_AUDITED_UTXOS_LIMIT = 100;

/** Mirrors Rust `AuditedUtxosAttestation::message`, the digest binds every requested hash in order. */
export function auditedUtxosAttestation(
  input: Readonly<{
    target: RingReadTarget;
    timestamp: bigint;
    nonce: Bytes32;
    utxoHashes: readonly Bytes32[];
    wallet?: Address;
  }>,
): Uint8Array {
  const joined = new Uint8Array(32 * input.utxoHashes.length);
  input.utxoHashes.forEach((utxoHash, index) => joined.set(utxoHash, 32 * index));
  return encoder.encode(
    [
      AUDITED_UTXOS_DOMAIN,
      ...targetLines(input.target),
      `timestamp: ${input.timestamp}`,
      `nonce: ${base64Decoder.decode(input.nonce)}`,
      `utxos: ${base64Decoder.decode(sha256(joined))}`,
      ...walletLines(input.wallet),
    ].join("\n"),
  );
}

/** Mirrors Rust `GetAuditedUtxosRequest` after `AuditedUtxosRequest::sign`. */
export interface SignedAuditedUtxosRead {
  readonly ringProgramId: Address;
  /** Answers come back in the same order. */
  readonly utxoHashes: readonly Bytes32[];
  /** Narrows the read to the wallet's notes. */
  readonly wallet?: Address;
  readonly reader: Uint8Array;
  readonly timestamp: bigint;
  readonly nonce: Bytes32;
  readonly signature: Uint8Array | WebAuthnSignature;
}

/** Mirrors Rust `AuditedUtxosRequest`, one to `RING_AUDITED_UTXOS_LIMIT` hashes. */
export class RingAuditedUtxosRequest extends AttestationRequest {
  readonly #target: RingReadTarget;
  readonly #utxoHashes: readonly Bytes32[];
  #wallet?: Address;

  private constructor(target: RingReadTarget, utxoHashes: readonly Bytes32[]) {
    super();
    this.#target = checkedReadTarget(target);
    this.#utxoHashes = utxoHashes;
  }

  static read(target: RingReadTarget, utxoHashes: readonly Bytes32[]): RingAuditedUtxosRequest {
    return new RingAuditedUtxosRequest(target, checkedUtxoHashes(utxoHashes));
  }

  withWallet(wallet: Address): this {
    addressBytes(wallet, "wallet");
    this.#wallet = wallet;
    return this;
  }

  async sign(signer: RingReadSigner): Promise<SignedAuditedUtxosRead> {
    const { timestamp, nonce } = this.stamp();
    const signature = await signer.sign(
      auditedUtxosAttestation({
        target: this.#target,
        timestamp,
        nonce,
        utxoHashes: this.#utxoHashes,
        ...(this.#wallet === undefined ? {} : { wallet: this.#wallet }),
      }),
    );
    return Object.freeze({
      ringProgramId: this.#target.ringProgramId,
      utxoHashes: this.#utxoHashes,
      ...(this.#wallet === undefined ? {} : { wallet: this.#wallet }),
      reader: signer.reader,
      timestamp,
      nonce,
      signature,
    });
  }
}

/** Mirrors Rust `DecryptedOutputContext`, the tree is the canonical PDA of `treeId`. */
export interface DecryptedOutputContext {
  readonly treeId: number;
  readonly leafIndex: bigint;
  readonly hash: Bytes32;
}

/** Mirrors Rust `DecryptedOutput`, a slot whose rebuilt opening reaches its published commitment. */
export interface DecryptedRingOutput {
  readonly slotIndex: number;
  readonly outputContext: DecryptedOutputContext;
  readonly opening: OutputOpening;
}

/** A public settlement leg. Value left the ring to a plain account. */
export interface DecryptedRingWithdrawal {
  readonly screenedOwner?: Address;
  /** A token account for an SPL leg, a wallet for a SOL leg. */
  readonly recipient: Address;
  /** The mint of an SPL leg or the native SOL mint. */
  readonly asset: Address;
  readonly amount: bigint;
}

/** Rust `DecryptedDefaultInput`. */
export type DecryptedRingDefaultInput = AuditedEntryInput;

/** Mirrors Rust `DecryptedTransaction`. */
export interface DecryptedRingTransaction {
  readonly slot: bigint;
  readonly signature: Signature;
  /** Ordinal of the confirmed SPP event within this Solana transaction. */
  readonly eventIndex: number;
  readonly txViewingPublicKey: X25519PublicKey;
  /** Published order, a slot sits here, in `payloadErrors`, or in both when its record stands and its payload failed. */
  readonly outputs: readonly DecryptedRingOutput[];
  readonly payloadErrors: readonly OutputPayloadError[];
  /** `R`, the notes the proof spent, their nullifiers sit among `nullifiers` at unnamed positions. */
  readonly realInputs: number;
  /** The default-ring notes among the `R`, in note order. */
  readonly defaultInputs: readonly DecryptedRingDefaultInput[];
  readonly nullifiers: readonly Bytes32[];
  /** The input owners of a confidential transfer, by owner signature or in proof. */
  readonly spenders: readonly Address[];
  /** Required signers, fee payer first. */
  readonly signers: readonly Address[];
  /** Empty when nothing left the ring. */
  readonly withdrawals: readonly DecryptedRingWithdrawal[];
  readonly vouchedBy?: Address;
}

/** Mirrors Rust `SkippedReason`. */
export type SkippedReason = "missingAuditorMessage" | "invalidAuditData";

export interface SkippedRingTransaction {
  readonly slot: bigint;
  readonly signature: Signature;
  readonly eventIndex: number;
  readonly reason: SkippedReason;
}

export interface DecryptedRingTransactionsPage {
  readonly slot: bigint;
  readonly blockTime: bigint;
  readonly items: readonly DecryptedRingTransaction[];
  readonly skipped: readonly SkippedRingTransaction[];
  readonly cursor?: Uint8Array;
}

/** Mirrors Rust `AuditedMergeRecord`, an audit record of inherited fields and no wallet opening. */
export interface AuditedRingMerge {
  readonly slot: bigint;
  readonly signature: Signature;
  readonly eventIndex: number;
  readonly outputCommitment: Bytes32;
  readonly ownerKey: Bytes32;
  readonly asset: Bytes32;
  readonly mint: Address;
  readonly amount: bigint;
  readonly realNullifiers: readonly Bytes32[];
}

/** Mirrors Rust `GetAuditedMergesResponse`. */
export interface AuditedMergesPage {
  readonly slot: bigint;
  readonly blockTime: bigint;
  readonly opened: readonly AuditedRingMerge[];
  readonly cursor?: Uint8Array;
}

/** Mirrors Rust `AuditedUtxoView`, a UTXO the auditor opened and the slot that created it. */
export interface RingAuditedUtxo {
  readonly utxoHash: Bytes32;
  readonly utxo: KnownUtxo;
  readonly slot: bigint;
  readonly signature: Signature;
  readonly eventIndex: number;
  readonly slotIndex: number;
}

export interface RingAuditedUtxosPage {
  readonly slot: bigint;
  readonly blockTime: bigint;
  /** In request order, `undefined` for a UTXO the service has not opened. */
  readonly utxos: readonly (RingAuditedUtxo | undefined)[];
}

/** One to `RING_AUDITED_UTXOS_LIMIT` 32-byte hashes. */
export function checkedUtxoHashes(value: unknown): readonly Bytes32[] {
  if (
    !Array.isArray(value) ||
    value.length === 0 ||
    value.length > RING_AUDITED_UTXOS_LIMIT ||
    !value.every((entry) => entry instanceof Uint8Array && entry.length === 32)
  )
    throw new RingError("RING_READ_LIMIT", {
      details: {
        name: "utxoHashes",
        length: Array.isArray(value) ? value.length : -1,
      },
    });
  return Object.freeze(value.map((entry: Uint8Array) => new Uint8Array(entry) as Bytes32));
}
export const SIGNED_UTXOS_REQUIRED_FIELDS = new Set([
  "ringProgramId",
  "utxoHashes",
  "reader",
  "timestamp",
  "nonce",
  "signature",
]);
export const SIGNED_UTXOS_FIELDS = new Set([...SIGNED_UTXOS_REQUIRED_FIELDS, "wallet"]);
