/** The transfer key catalog, Rust `key_catalog.rs` and `Shapes` in `prover/circuits/transfer`. */
import type { RingRail } from "../transaction/transact.js";
import { TRANSFER_SHAPES } from "./ring-transfer-shapes.js";

export { TRANSFER_SHAPES };

/** One transfer key, Rust `TransferShape`. */
export interface TransferShape {
  readonly outputs: number;
  /** The widest published input count the key serves. */
  readonly inputs: number;
  readonly withdrawals: number;
  /** Owner rows the key opens. */
  readonly owners: number;
  /** One evidence slot per input, the key also spends default-ring notes. */
  readonly entry: boolean;
  /** A public record and the records under the owner's salt and the ring salt fit. */
  readonly wide: boolean;
  /** Header wide bit, clear on narrow entry keys whose pools stay wide. */
  readonly frameWide: boolean;
  /** The most tracking rows one inline transaction on the key opens. */
  readonly tracking: number;
  /** Excludes input and public slot assets. */
  readonly records: number;
  readonly pools: PolicyPoolCapacity;
}

/** What names a frame's key, nothing else moves a record's counter, Rust `FrameKey`. */
export interface FrameKey {
  readonly rail: RingRail;
  /** `n`, the frame's nullifier count. */
  readonly inputs: number;
  readonly outputs: number;
  readonly spendsDefaultRing: boolean;
  readonly wide: boolean;
}

/** `undefined` when the catalog keys none, Rust `FrameKey::shape`. */
export function frameKeyShape(key: FrameKey): TransferShape | undefined {
  // SPP's authority rail holds every input to the ring.
  if (key.spendsDefaultRing && (key.rail === "delegate" || key.rail === "anonymousDelegate")) {
    return undefined;
  }
  return TRANSFER_SHAPES.find(
    (entry) =>
      entry.entry === key.spendsDefaultRing &&
      entry.frameWide === key.wide &&
      key.outputs <= entry.outputs &&
      key.inputs <= entry.inputs,
  );
}

/** A transfer key, Rust `RingCircuit::Transfer`. */
export interface TransferKeyCircuit {
  readonly family: "transfer";
  readonly outputs: number;
  /** The widest published input count. */
  readonly class: number;
  readonly withdrawals: number;
  readonly owners: number;
  readonly entry: boolean;
  /** A public record and the records under the owner's salt and the ring salt fit. */
  readonly wide: boolean;
}

/** A key's record pool widths, Rust `RecordPools`. */
export interface PolicyPoolCapacity {
  readonly own: number;
  readonly curator: number;
  /** Tracking openings, Rust `RecordPools::tracking`. */
  readonly tracking: number;
}

/** What selects a transfer key, Rust `TransferSpend`. */
export interface TransferSpend {
  readonly rail: RingRail;
  /** Published inputs, the spent notes. */
  readonly inputs: number;
  readonly outputs: number;
  readonly spendsDefaultRing: boolean;
  /** Public withdrawal legs. */
  readonly legs: number;
  /** Distinct live input owners. */
  readonly owners: number;
  /** Distinct records the spend opens in each pool, a salted read takes a row beside the public one. */
  readonly pools: PolicyPoolCapacity;
  readonly wide: boolean;
}

/** The wide flag follows the narrow key failing to hold the spend, Rust `TransferSpend::fitted`. */
export function fittedSpend(spend: TransferSpend): TransferSpend {
  const narrow = frameKeyShape({ ...spend, wide: false });
  return { ...spend, wide: narrow === undefined || !shapeHolds(narrow, spend) };
}

/** The one transfer key serving the spend, `undefined` when the catalog keys none. */
export function ringTransferCircuit(spend: TransferSpend): TransferKeyCircuit | undefined {
  const found = frameKeyShape(spend);
  return found === undefined || !shapeHolds(found, spend) ? undefined : shapeCircuit(found);
}

/** `S`, the owner signer slots of a key's statement chain, Go `Sections.Signers`. */
export function signerSlots(circuit: TransferKeyCircuit): number {
  return circuit.class;
}

/** Rust `RingCircuit::record_pools`, each pool takes as many subjects as records. */
export function transferPoolCapacity(circuit: TransferKeyCircuit): PolicyPoolCapacity {
  const row = TRANSFER_SHAPES.find((entry) => sameCircuit(shapeCircuit(entry), circuit));
  if (row === undefined) throw new RangeError(`no catalog key ${transferCircuitType(circuit)}`);
  return row.pools;
}

function sameCircuit(left: TransferKeyCircuit, right: TransferKeyCircuit): boolean {
  return (
    left.outputs === right.outputs &&
    left.class === right.class &&
    left.withdrawals === right.withdrawals &&
    left.owners === right.owners &&
    left.entry === right.entry &&
    left.wide === right.wide
  );
}

/** The circuit type the prover serves the key under, Go `Key.circuitType`. */
export function transferCircuitType(circuit: TransferKeyCircuit): string {
  const family = circuit.entry ? "transfer-entry" : circuit.wide ? "transfer-wide" : "transfer";
  return `custom-ring-${family}-${circuit.outputs}-in${circuit.class}-w${circuit.withdrawals}-s${circuit.owners}`;
}

/** Every transfer key in the catalog's order, Go `keys`. */
export function transferKeys(): readonly TransferKeyCircuit[] {
  return TRANSFER_SHAPES.map(shapeCircuit);
}

/** Rust `SPP_ADDRESSES`, the addresses SPP takes beside its nullifier PDAs. */
const SPP_ADDRESSES = 4;
/** Rust `RING_ADDRESSES`, ring program, policy, config, own, curator and tracking registries. */
const RING_ADDRESSES = 6;
import { MAX_TRANSACTION_ADDRESSES } from "../transaction/v1-limits.js";

export { MAX_TRANSACTION_ADDRESSES };

/** What takes a v1 transaction's addresses beside its owner signers, Rust `TransferAddresses`. */
export interface TransferAddresses {
  /** The shape's nullifier PDAs. */
  readonly nInputs: number;
  readonly coSigners: number;
  readonly legs: number;
}

/** Rust `TransferAddresses::owner_signers`, a co-signer takes its record too. */
export function ownerSignerCapacity(addresses: TransferAddresses): number {
  const taken =
    SPP_ADDRESSES + RING_ADDRESSES + addresses.nInputs + 2 * addresses.coSigners + addresses.legs;
  return Math.max(0, MAX_TRANSACTION_ADDRESSES - taken);
}

function shapeCircuit(entry: TransferShape): TransferKeyCircuit {
  return Object.freeze({
    family: "transfer",
    outputs: entry.outputs,
    class: entry.inputs,
    withdrawals: entry.withdrawals,
    owners: entry.owners,
    entry: entry.entry,
    wide: entry.wide,
  });
}

/** Rust `TransferShape::holds`. */
function shapeHolds(entry: TransferShape, spend: TransferSpend): boolean {
  const { pools } = entry;
  return (
    Number.isInteger(spend.inputs) &&
    spend.inputs >= 1 &&
    spend.inputs <= entry.inputs &&
    spend.legs <= entry.withdrawals &&
    spend.owners <= entry.owners &&
    spend.pools.own <= pools.own &&
    spend.pools.curator <= pools.curator &&
    spend.pools.tracking <= pools.tracking
  );
}
