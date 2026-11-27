import {
  type RingWalletActivity,
  type RingWalletView,
  type RingWalletNoteStatus,
  type RingWalletStatusPage,
  checkedSnapshotCursor,
  type RingWalletNote,
  type RingSpendCoverage,
  type RingSpendCoverageFrame,
  type RingSpendCoverageRow,
} from "./wallet-reads.js";
import {
  record,
  list,
  hash,
  invalid,
  u64,
  signature,
  u16,
  canonicalHash,
  namedMint,
  base64,
  u32,
} from "./wire.js";
import {
  decodeOutputs,
  decodeWithdrawals,
  decodeDefaultInputs,
  optionalDataHashes,
} from "./audit-decoders.js";
import { checkedReadCursor } from "./signed-read.js";
import { defined } from "../../protocol/defined.js";

export function decodeWalletActivity(wire: Record<string, unknown>): RingWalletActivity {
  const view = record(wire["view"], "view");
  const outputs = decodeOutputs(view["outputs"], "view.outputs");
  const withdrawals = decodeWithdrawals(view["withdrawals"], "view.withdrawals");
  let decoded: RingWalletView;
  if (view["role"] === "sender") {
    decoded = Object.freeze({
      role: "sender",
      spent: Object.freeze(
        list(view["spent"], "view.spent").map((nullifier, index) =>
          hash(nullifier, `view.spent[${index}]`),
        ),
      ),
      defaultInputs: decodeDefaultInputs(view["defaultInputs"], "view.defaultInputs"),
      outputs,
      withdrawals,
    });
  } else if (view["role"] === "recipient" && outputs.length > 0) {
    decoded = Object.freeze({ role: "recipient", outputs, withdrawals });
  } else {
    throw invalid("view.role");
  }
  return Object.freeze({
    slot: u64(wire["slot"], "slot"),
    signature: signature(wire["txSignature"], "txSignature"),
    eventIndex: u16(wire["eventIndex"], "eventIndex"),
    view: decoded,
  });
}

const WALLET_NOTE_STATUSES: readonly RingWalletNoteStatus[] = [
  "spent",
  "unspent",
  "unknown",
  "exposedIfSpent",
];

export function decodeWalletStatus(wire: Record<string, unknown>): RingWalletStatusPage {
  const cursor = wire["cursor"];
  const enrollment = wire["enrollment"];
  if (enrollment !== "notEnrolled" && enrollment !== "applied") throw invalid("enrollment");
  return Object.freeze({
    enrollment,
    ...(wire["snapshotExpiresAt"] == null
      ? {}
      : { snapshotExpiresAt: u64(wire["snapshotExpiresAt"], "snapshotExpiresAt") }),
    historyThrough: u64(wire["historyThrough"], "historyThrough"),
    balances: Object.freeze(
      list(wire["balances"], "balances").map((entry, index) => {
        const path = `balances[${index}]`;
        const row = record(entry, path);
        const asset = canonicalHash(row["asset"], `${path}.asset`);
        const mint = row["mint"] == null ? undefined : namedMint(row, asset, path);
        return Object.freeze({
          asset,
          ...defined({ mint }),
          amount: u64(row["amount"], `${path}.amount`),
        });
      }),
    ),
    notes: Object.freeze(
      list(wire["notes"], "notes").map((entry, index) =>
        decodeWalletNote(record(entry, `notes[${index}]`), `notes[${index}]`),
      ),
    ),
    ...(cursor == null ? {} : { cursor: checkedSnapshotCursor(base64(cursor, "cursor")) }),
  });
}

function decodeWalletNote(row: Record<string, unknown>, path: string): RingWalletNote {
  const status = WALLET_NOTE_STATUSES.find((known) => known === row["status"]);
  if (status === undefined) throw invalid(`${path}.status`);
  const asset = canonicalHash(row["asset"], `${path}.asset`);
  const mint = row["mint"] == null ? undefined : namedMint(row, asset, path);
  return Object.freeze({
    utxoHash: canonicalHash(row["utxoHash"], `${path}.utxoHash`),
    status,
    ...(row["nullifierPk"] == null
      ? {}
      : { nullifierPk: canonicalHash(row["nullifierPk"], `${path}.nullifierPk`) }),
    asset,
    ...defined({ mint }),
    amount: u64(row["amount"], `${path}.amount`),
    blinding: canonicalHash(row["blinding"], `${path}.blinding`),
    ...optionalDataHashes(row, path, canonicalHash),
    treeId: u16(row["treeId"], `${path}.treeId`),
    leafIndex: u64(row["leafIndex"], `${path}.leafIndex`),
    slot: u64(row["slot"], `${path}.slot`),
    signature: signature(row["txSignature"], `${path}.txSignature`),
    eventIndex: u16(row["eventIndex"], `${path}.eventIndex`),
    slotIndex: u32(row["slotIndex"], `${path}.slotIndex`),
  });
}

export function decodeSpendCoverage(wire: Record<string, unknown>): RingSpendCoverage {
  const frame = (value: unknown, path: string): RingSpendCoverageFrame => {
    const row = record(value, path);
    return Object.freeze({
      slot: u64(row["slot"], `${path}.slot`),
      signature: signature(row["txSignature"], `${path}.txSignature`),
      eventIndex: u16(row["eventIndex"], `${path}.eventIndex`),
    });
  };
  const rows = (name: string): readonly RingSpendCoverageRow[] =>
    Object.freeze(
      list(wire[name], name).map((entry, index) => {
        const path = `${name}[${index}]`;
        const row = record(entry, path);
        return Object.freeze({
          nullifier: hash(row["nullifier"], `${path}.nullifier`),
          frame: frame(row["frame"], `${path}.frame`),
        });
      }),
    );
  const totals = wire["totals"] == null ? undefined : record(wire["totals"], "totals");
  return Object.freeze({
    ...(wire["positionSlot"] == null
      ? {}
      : { positionSlot: u64(wire["positionSlot"], "positionSlot") }),
    historyThrough: u64(wire["historyThrough"], "historyThrough"),
    ...(totals === undefined
      ? {}
      : {
          totals: Object.freeze({
            observed: u64(totals["observed"], "totals.observed"),
            padding: u64(totals["padding"], "totals.padding"),
            resolved: u64(totals["resolved"], "totals.resolved"),
          }),
        }),
    unclassified: rows("unclassified"),
    unreadable: Object.freeze(
      list(wire["unreadable"], "unreadable").map((entry, index) =>
        frame(entry, `unreadable[${index}]`),
      ),
    ),
    integrityFaults: rows("integrityFaults"),
    ...(wire["cursor"] == null
      ? {}
      : { cursor: checkedReadCursor(base64(wire["cursor"], "cursor")) }),
  });
}
