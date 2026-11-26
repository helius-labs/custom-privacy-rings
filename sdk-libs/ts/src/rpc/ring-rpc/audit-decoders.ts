import { type Bytes32, MAX_MERGE_INPUTS } from "@heliuslabs/spr/interface";
import { equalBytes, assetField } from "@heliuslabs/spr/internal";
import type {
  AuditedUtxo,
  KnownUtxo,
  OutputOpening,
  OutputPayloadError,
  OutputPayloadErrorKind,
} from "../../operations/audit/audit.js";
import type {
  SkippedReason,
  DecryptedRingTransaction,
  DecryptedRingOutput,
  DecryptedRingWithdrawal,
  DecryptedRingDefaultInput,
  AuditedRingMerge,
  SkippedRingTransaction,
} from "./audit-reads.js";
import {
  list,
  hash,
  invalid,
  u64,
  signature,
  u16,
  x25519Key,
  address,
  record,
  u32,
  canonicalHash,
  exactRecord,
  namedMint,
} from "./wire.js";

const SKIPPED_REASONS: readonly SkippedReason[] = Object.freeze([
  "missingAuditorMessage",
  "invalidAuditData",
]);

/** Structure only, the openings stay as the service rebuilt them. */
export function decodeTransaction(wire: Record<string, unknown>): DecryptedRingTransaction {
  const outputs = decodeOutputs(wire["outputs"]);
  const payloadErrors = decodePayloadErrors(wire["payloadErrors"]);
  checkSlotCoverage(
    outputs.map((output) => output.slotIndex),
    payloadErrors.map((error) => error.slotIndex),
  );
  const nullifiers = list(wire["nullifiers"], "nullifiers").map((nullifier, index) =>
    hash(nullifier, `nullifiers[${index}]`),
  );
  const defaultInputs = decodeDefaultInputs(wire["defaultInputs"], "defaultInputs");
  const realInputs = wire["realInputs"];
  if (typeof realInputs !== "number" || !Number.isInteger(realInputs) || realInputs < 0)
    throw invalid("realInputs");
  // Every note takes one nullifier, and each default-ring note is one of the `R`.
  if (realInputs > nullifiers.length || defaultInputs.length > realInputs)
    throw invalid("realInputs");
  return Object.freeze({
    slot: u64(wire["slot"], "slot"),
    signature: signature(wire["txSignature"], "txSignature"),
    eventIndex: u16(wire["eventIndex"], "eventIndex"),
    txViewingPublicKey: x25519Key(wire["txViewingPk"], "txViewingPk"),
    outputs,
    payloadErrors,
    realInputs,
    defaultInputs,
    nullifiers: Object.freeze(nullifiers),
    spenders: Object.freeze(
      wire["spenders"] === undefined
        ? []
        : list(wire["spenders"], "spenders").map((key, index) =>
            address(key, `spenders[${index}]`),
          ),
    ),
    signers: Object.freeze(
      list(wire["signers"], "signers").map((key, index) => address(key, `signers[${index}]`)),
    ),
    withdrawals: decodeWithdrawals(wire["withdrawals"]),
    ...(wire["vouchedBy"] === undefined
      ? {}
      : { vouchedBy: address(wire["vouchedBy"], "vouchedBy") }),
  });
}

export function decodeOutputs(value: unknown, path = "outputs"): readonly DecryptedRingOutput[] {
  return Object.freeze(
    list(value, path).map((entry, index) =>
      decodeOutput(record(entry, `${path}[${index}]`), `${path}[${index}]`),
    ),
  );
}

function decodePayloadErrors(
  value: unknown,
  path = "payloadErrors",
): readonly OutputPayloadError[] {
  return Object.freeze(
    list(value, path).map((entry, index) => {
      const error = record(entry, `${path}[${index}]`);
      return Object.freeze({
        slotIndex: u32(error["slotIndex"], `${path}[${index}].slotIndex`),
        kind: payloadErrorKind(error["kind"], `${path}[${index}].kind`),
      });
    }),
  );
}

export function decodeWithdrawals(
  value: unknown,
  path = "withdrawals",
): readonly DecryptedRingWithdrawal[] {
  return Object.freeze(
    list(value, path).map((entry, index) => {
      const legPath = `${path}[${index}]`;
      const leg = record(entry, legPath);
      return Object.freeze({
        ...(leg["screenedOwner"] === undefined
          ? {}
          : {
              screenedOwner: address(leg["screenedOwner"], `${legPath}.screenedOwner`),
            }),
        recipient: address(leg["recipient"], `${legPath}.recipient`),
        asset: address(leg["asset"], `${legPath}.asset`),
        amount: u64(leg["amount"], `${legPath}.amount`),
      });
    }),
  );
}

/** Rust `AuditedTransaction::place`, a kept record whose payload failed sits in both lists. */
function checkSlotCoverage(outputs: readonly number[], errors: readonly number[]): void {
  if (new Set(outputs).size !== outputs.length) throw invalid("outputs");
  if (new Set(errors).size !== errors.length) throw invalid("payloadErrors");
  const slots = new Set([...outputs, ...errors]);
  if ([...slots].some((index) => index >= slots.size)) throw invalid("payloadErrors");
}

function decodeOutput(wire: Record<string, unknown>, path: string): DecryptedRingOutput {
  const context = record(wire["outputContext"], `${path}.outputContext`);
  const treeId = u32(context["treeId"], `${path}.outputContext.treeId`);
  if (treeId > 0xffff) throw invalid(`${path}.outputContext.treeId`);
  const kind = wire["kind"];
  const utxo = wire["utxo"];
  const tag = wire["tag"];
  let opening: OutputOpening;
  if (kind === "ring" && utxo !== undefined && tag !== undefined) {
    opening = Object.freeze({
      kind,
      utxo: decodeDecryptedUtxo(record(utxo, `${path}.utxo`), `${path}.utxo`),
      ownerTag: Object.freeze({ kind: "view", tag: hash(tag, `${path}.tag`) }),
    });
  } else if (kind === "exit" && utxo !== undefined && tag === undefined) {
    opening = Object.freeze({
      kind,
      utxo: decodeDecryptedUtxo(record(utxo, `${path}.utxo`), `${path}.utxo`),
    });
  } else if (kind === "dummy" && utxo === undefined && tag === undefined) {
    opening = Object.freeze({ kind });
  } else {
    throw invalid(`${path}.kind`);
  }
  return Object.freeze({
    slotIndex: u32(wire["slotIndex"], `${path}.slotIndex`),
    outputContext: Object.freeze({
      treeId,
      leafIndex: u64(context["leafIndex"], `${path}.outputContext.leafIndex`),
      hash: hash(context["hash"], `${path}.outputContext.hash`),
    }),
    opening,
  });
}

/** Rust `DecryptedUtxo`, `nullifierPk` absent until the auditor reads the owner's registered key. */
function decodeDecryptedUtxo(wire: Record<string, unknown>, path: string): AuditedUtxo {
  const nullifierPk = wire["nullifierPk"];
  const fields = utxoFields(wire, path, canonicalHash);
  if (fields.mint !== undefined && !equalBytes(assetField(fields.mint), fields.asset))
    throw invalid(`${path}.mint`);
  return Object.freeze({
    ...fields,
    ...(nullifierPk === undefined
      ? {}
      : { nullifierPk: canonicalHash(nullifierPk, `${path}.nullifierPk`) }),
  });
}

const KNOWN_UTXO_FIELDS: readonly string[] = [
  "ownerKey",
  "nullifierPk",
  "asset",
  "mint",
  "amount",
  "blinding",
  "dataHash",
  "ringDataHash",
  "treeId",
];

/** Rust `DecryptedKnownUtxo`, no `nullifierPk` for an unresolved owner. */
export function decodeKnownUtxo(wire: Record<string, unknown>, path: string): KnownUtxo {
  const nullifierPk = wire["nullifierPk"];
  const unknown = Object.keys(wire).find((field) => !KNOWN_UTXO_FIELDS.includes(field));
  if (unknown !== undefined) throw invalid(`${path}.${unknown}`);
  const fields = utxoFields(wire, path, canonicalHash);
  // The commitment binds the asset field alone, a named mint must derive it.
  if (fields.mint !== undefined && !equalBytes(assetField(fields.mint), fields.asset))
    throw invalid(`${path}.mint`);
  return Object.freeze({
    ...fields,
    owner:
      nullifierPk === undefined
        ? Object.freeze({ kind: "unresolved" })
        : Object.freeze({
            kind: "keyed",
            nullifierPk: canonicalHash(nullifierPk, `${path}.nullifierPk`),
          }),
    treeId: u16(wire["treeId"], `${path}.treeId`),
  });
}

function utxoFields(
  wire: Record<string, unknown>,
  path: string,
  field: (input: unknown, path: string) => Bytes32,
): Omit<AuditedUtxo, "nullifierPk"> {
  const mint = wire["mint"];
  return {
    ownerKey: hash(wire["ownerKey"], `${path}.ownerKey`),
    asset: field(wire["asset"], `${path}.asset`),
    ...(mint === undefined ? {} : { mint: address(mint, `${path}.mint`) }),
    amount: u64(wire["amount"], `${path}.amount`),
    blinding: field(wire["blinding"], `${path}.blinding`),
    ...optionalDataHashes(wire, path, field),
  };
}

export function optionalDataHashes(
  wire: Record<string, unknown>,
  path: string,
  field: (input: unknown, path: string) => Bytes32,
): Readonly<{ dataHash?: Bytes32; ringDataHash?: Bytes32 }> {
  const dataHash = wire["dataHash"];
  const ringDataHash = wire["ringDataHash"];
  return {
    ...(dataHash === undefined ? {} : { dataHash: field(dataHash, `${path}.dataHash`) }),
    ...(ringDataHash === undefined
      ? {}
      : { ringDataHash: field(ringDataHash, `${path}.ringDataHash`) }),
  };
}

/** Absent reads empty, Rust skips an empty list. */
export function decodeDefaultInputs(
  value: unknown,
  path: string,
): readonly DecryptedRingDefaultInput[] {
  if (value === undefined) return Object.freeze([]);
  return Object.freeze(
    list(value, path).map((entry, index) => {
      const at = `${path}[${index}]`;
      const wire = exactRecord(entry, DEFAULT_INPUT_FIELDS, DEFAULT_INPUT_FIELDS, at);
      const asset = canonicalHash(wire["asset"], `${at}.asset`);
      return Object.freeze({
        ownerKey: hash(wire["ownerKey"], `${at}.ownerKey`),
        nullifierPk: canonicalHash(wire["nullifierPk"], `${at}.nullifierPk`),
        asset,
        mint: namedMint(wire, asset, at),
        amount: u64(wire["amount"], `${at}.amount`),
        commitment: canonicalHash(wire["commitment"], `${at}.commitment`),
      });
    }),
  );
}

export function decodeAuditedMerge(value: unknown, path: string): AuditedRingMerge {
  const wire = exactRecord(value, AUDITED_MERGE_FIELDS, AUDITED_MERGE_REQUIRED_FIELDS, path);
  const asset = canonicalHash(wire["asset"], `${path}.asset`);
  const realNullifiers =
    wire["realNullifiers"] === undefined
      ? []
      : mergeCommitments(wire["realNullifiers"], `${path}.realNullifiers`);
  return Object.freeze({
    ...mergeRow(wire, path),
    ownerKey: hash(wire["ownerKey"], `${path}.ownerKey`),
    asset,
    mint: namedMint(wire, asset, path),
    amount: u64(wire["amount"], `${path}.amount`),
    realNullifiers,
  });
}

function mergeCommitments(value: unknown, path: string): readonly Bytes32[] {
  const commitments = list(value, path);
  if (commitments.length > MAX_MERGE_INPUTS) throw invalid(path);
  return Object.freeze(
    commitments.map((commitment, index) => canonicalHash(commitment, `${path}[${index}]`)),
  );
}

function mergeRow(
  wire: Record<string, unknown>,
  path: string,
): Pick<AuditedRingMerge, "slot" | "signature" | "eventIndex" | "outputCommitment"> {
  return {
    slot: u64(wire["slot"], `${path}.slot`),
    signature: signature(wire["txSignature"], `${path}.txSignature`),
    eventIndex: u16(wire["eventIndex"], `${path}.eventIndex`),
    outputCommitment: canonicalHash(wire["outputCommitment"], `${path}.outputCommitment`),
  };
}

/** Rust `OutputPayloadErrorKind` as the ring rpc serializes it. */
export const RING_PAYLOAD_ERROR_KINDS: readonly OutputPayloadErrorKind[] = Object.freeze([
  "disclosure",
  "commitment",
]);

function payloadErrorKind(value: unknown, path: string): OutputPayloadErrorKind {
  const kind = RING_PAYLOAD_ERROR_KINDS.find((candidate) => candidate === value);
  if (kind === undefined) throw invalid(path);
  return kind;
}

export function decodeSkipped(wire: Record<string, unknown>): SkippedRingTransaction {
  const reason = SKIPPED_REASONS.find((candidate) => candidate === wire["reason"]);
  if (reason === undefined) throw invalid("reason");
  return Object.freeze({
    slot: u64(wire["slot"], "slot"),
    signature: signature(wire["txSignature"], "txSignature"),
    eventIndex: u16(wire["eventIndex"], "eventIndex"),
    reason,
  });
}
const DEFAULT_INPUT_FIELDS = new Set([
  "ownerKey",
  "nullifierPk",
  "asset",
  "mint",
  "amount",
  "commitment",
]);
export const AUDITED_MERGES_PAGE_FIELDS = new Set(["opened", "cursor"]);
const AUDITED_MERGE_REQUIRED_FIELDS = new Set([
  "slot",
  "txSignature",
  "eventIndex",
  "outputCommitment",
  "ownerKey",
  "asset",
  "mint",
  "amount",
]);
const AUDITED_MERGE_FIELDS = new Set([...AUDITED_MERGE_REQUIRED_FIELDS, "realNullifiers"]);
