/** Regions of the ring auditor stream and the output record, Rust `custom_ring_interface::audit_stream`. */
import { InterfaceError } from "@heliuslabs/spr/interface";

/** Rust `TRANSFER_OUTPUT_CAPACITIES`, the output counts of the confidential keys. */
export const TRANSFER_OUTPUT_CAPACITIES: readonly number[] = Object.freeze([2, 4, 8, 16]);
/** Rust `PACK_BE_CHUNK_BYTES`, the bytes of one packed field. */
export const DISCLOSURE_CHUNK_BYTES = 31;
export const STREAM_BLOCK_LEN = 16;
export const STREAM_FIRST_COUNTER = 2;
/** The sealed `tx_viewing_sk`, the envelope's first bytes. */
export const ENVELOPE_KEY_LEN = 32;
/** `R`, the spent notes. */
export const REAL_INPUT_COUNT_LEN = 1;
/** `tx_viewing_sk ‖ R`, the same on every key. */
export const ENVELOPE_HEAD_LEN = ENVELOPE_KEY_LEN + REAL_INPUT_COUNT_LEN;
/** The default note bitmap is u64, a wider input class needs a new layout. */
export const MAX_INPUT_CLASS = 64;
/** Keys with entry capacity alone, bit `k` set when note `k` is a default-ring note. */
export const DEFAULT_NOTE_BITS_LEN = 8;
/** `kind ‖ u64be(amount) ‖ owner_key ‖ mint ‖ be32(blinding)`, a dummy or ring output slot publishes it under a clear form. */
export const RING_RECORD_LEN = 105;
/** The ring record then `be32(nullifier_pk)`, an exit publishes it under a clear form. */
export const EXIT_RECORD_LEN = RING_RECORD_LEN + 32;
/** The plaintext every transfer slot seals, an exit's key then its `data_hash`, any other slot's two hashes. */
export const TRANSFER_RECORD_LEN = EXIT_RECORD_LEN + 32;
/** `head ‖ be32(nullifier_pk) ‖ be32(seed)`. */
export const MERGE_RECORD_LEN = EXIT_RECORD_LEN + 32;
/** `eph_sk ‖ V`, sealed to the auditor. */
export const MERGE_ENVELOPE_LEN = 64;
/** `owner_key ‖ be32(nullifier_pk) ‖ u64be(amount) ‖ mint ‖ be32(input_commitment)`. */
export const ENTRY_RECORD_LEN = 136;
/** `0x04 ‖ eph_u` ahead of the auditor ciphertexts. */
export const TAGGED_KEY_LEN = 33;
/** One slot's sealed recipient key `V_i`, the raw `u`. */
export const RECIPIENT_KEY_LEN = 32;
/** Each recipient key keeps its own counters, no slot shares a block with the next. */
export const RECIPIENT_KEY_BLOCKS = 2;
/** `u16be(len)` ahead of a payload. */
export const PAYLOAD_LENGTH_LEN = 2;

/** A record's kind without its zero-key bit, the circuit derives it from the slot's domain and ring id, Rust `RecordKind`. */
export const RecordKind = Object.freeze({ dummy: 0, ring: 1, exit: 2 } as const);
export type RecordKind = (typeof RecordKind)[keyof typeof RecordKind];

/** The hashes a frame's records publish at one length per kind, the proof holds an unpublished hash to zero, Rust `RecordForm`. */
export interface RecordForm {
  /** Every record publishes `data_hash`. */
  readonly data: boolean;
  /** Every non-exit record publishes `ring_data_hash`. */
  readonly ringData: boolean;
}

export const CLEAR_RECORD_FORM: RecordForm = Object.freeze({ data: false, ringData: false });

const RECORD_FORMS: readonly RecordForm[] = Object.freeze(
  [false, true].flatMap((data) =>
    [false, true].map((ringData) => Object.freeze({ data, ringData })),
  ),
);

/** The prefix of `TRANSFER_RECORD_LEN` a slot of `kind` publishes, a dummy as a ring slot, Rust `RecordForm::published_len`. */
export function publishedRecordLen(form: RecordForm, kind: RecordKind): number {
  return kind === RecordKind.exit
    ? EXIT_RECORD_LEN + (form.data ? 32 : 0)
    : RING_RECORD_LEN + (form.data ? 32 : 0) + (form.ringData ? 32 : 0);
}

/** A data frame's payload follows the record at its length prefix or longer, any other body ends at the record, Rust `RecordForm::fits_recipient_body`. */
function fitsRecipientBody(form: RecordForm, bodyLength: number): boolean {
  const recordLength = publishedRecordLen(form, RecordKind.ring);
  return form.data ? bodyLength >= recordLength + PAYLOAD_LENGTH_LEN : bodyLength === recordLength;
}

/** The forms a recipient body of `bodyLength` can hold, a reader without the frame tries each against the commitment, Rust `RecordForm::of_recipient_body`. */
export function recipientBodyForms(bodyLength: number): readonly RecordForm[] {
  return RECORD_FORMS.filter((form) => fitsRecipientBody(form, bodyLength));
}

/** The statement's layout element, Rust `AuditLayout`. */
export interface AuditLayout {
  /** The key carries default bits and one evidence entry per published input. */
  readonly entry: boolean;
  /** The frame publishes the voucher's sealed key after the slot keys. */
  readonly vouched: boolean;
  readonly records: RecordForm;
}

/** Bit 0 vouched, bit 1 entry, bit 2 data, bit 3 ring data. */
export function auditLayoutValue(layout: AuditLayout): number {
  const { records } = layout;
  if (
    typeof layout.entry !== "boolean" ||
    typeof layout.vouched !== "boolean" ||
    typeof records?.data !== "boolean" ||
    typeof records.ringData !== "boolean"
  )
    throw shapeError("layout");
  return (
    (layout.vouched ? 1 : 0) |
    (layout.entry ? 2 : 0) |
    (records.data ? 4 : 0) |
    (records.ringData ? 8 : 0)
  );
}

/** Rust `AuditLayout::split_keys`, a vouched region shorter than one key refused. */
export function splitSealedKeys(
  layout: Pick<AuditLayout, "vouched">,
  sealed: Uint8Array,
): Readonly<{ keys: Uint8Array; voucher?: Uint8Array }> {
  if (!layout.vouched) return { keys: sealed };
  const keysLen = sealed.length - RECIPIENT_KEY_LEN;
  if (keysLen < 0) throw shapeError("recipientKeys");
  return { keys: sealed.subarray(0, keysLen), voucher: sealed.subarray(keysLen) };
}

/** One key's envelope, every rail of a key shares one circuit, Rust `AuditKey`. */
export interface AuditKey {
  /** `C`, the widest published input count. */
  readonly class: number;
  /** Default bits and one evidence entry per published input. */
  readonly entry: boolean;
}

/** One stretch of the auditor keystream, no counter serves two regions, Rust `StreamRegion`. */
export interface AuditStreamRegion {
  readonly startCounter: number;
  readonly maxBlocks: number;
  /** Transmitted bytes, padding and unused blocks never leave the prover. */
  readonly length: number;
}

/** Every length of one transfer's auditor stream, a frame of any other length is refused, Rust `AuditStreamLayout`. */
export interface AuditStreamLayout {
  readonly key: AuditKey;
  /** `n`, the frame's nullifier count, never the key capacity or a private live count. */
  readonly inputs: number;
  /** `m`, the published slots. */
  readonly outputs: number;
  /** `N`, the key's output capacity, the recipient keys span every slot of it. */
  readonly capacity: number;
  /** The frame publishes the voucher's key. */
  readonly vouched: boolean;
  readonly records: RecordForm;
}

/** The statement's layout element of a stream, Rust `AuditStreamLayout::layout`. */
export function auditLayoutOf(layout: AuditStreamLayout): AuditLayout {
  return Object.freeze({
    entry: layout.key.entry,
    vouched: layout.vouched,
    records: layout.records,
  });
}

/** Refuses an input count off the key, a key past the bitmap and an output count off the capacity. */
export function checkedStreamLayout(layout: AuditStreamLayout): AuditStreamLayout {
  const { key, inputs, outputs, capacity } = layout;
  const counted =
    Number.isInteger(inputs) &&
    inputs >= 1 &&
    inputs <= key.class &&
    key.class <= MAX_INPUT_CLASS &&
    typeof key.entry === "boolean" &&
    typeof layout.vouched === "boolean" &&
    typeof layout.records?.data === "boolean" &&
    typeof layout.records.ringData === "boolean" &&
    Number.isInteger(capacity) &&
    Number.isInteger(outputs) &&
    outputs >= 1 &&
    outputs <= capacity;
  if (!counted) throw shapeError("layout");
  return layout;
}

function headLen(key: AuditKey): number {
  return ENVELOPE_HEAD_LEN + (key.entry ? DEFAULT_NOTE_BITS_LEN : 0);
}

function entryLen(key: AuditKey): number {
  return key.entry ? ENTRY_RECORD_LEN : 0;
}

/** `E`, the same at every published input count, no recipient key's counter moves. */
export function envelopeBlocks(key: AuditKey): number {
  return Math.ceil((headLen(key) + entryLen(key) * key.class) / STREAM_BLOCK_LEN);
}

/** `K0`. */
export function firstRecipientKeyCounter(key: AuditKey): number {
  return STREAM_FIRST_COUNTER + envelopeBlocks(key);
}

/** Slots at or past `n` stay absent from the evidence. */
export function envelopeRegion(layout: AuditStreamLayout): AuditStreamRegion {
  return Object.freeze({
    startCounter: STREAM_FIRST_COUNTER,
    maxBlocks: envelopeBlocks(layout.key),
    length: headLen(layout.key) + entryLen(layout.key) * layout.inputs,
  });
}

/** Its counter fixed by the slot index alone, omission moves no region. */
export function recipientKeyRegion(layout: AuditStreamLayout, slot: number): AuditStreamRegion {
  return Object.freeze({
    startCounter: firstRecipientKeyCounter(layout.key) + slot * RECIPIENT_KEY_BLOCKS,
    maxBlocks: RECIPIENT_KEY_BLOCKS,
    length: slot < layout.outputs ? RECIPIENT_KEY_LEN : 0,
  });
}

/** The voucher's key on the blocks past the last slot's, Rust `AuditStreamLayout::voucher`. */
export function voucherRegion(
  layout: Pick<AuditStreamLayout, "key" | "capacity" | "vouched">,
): AuditStreamRegion {
  return Object.freeze({
    startCounter: firstRecipientKeyCounter(layout.key) + layout.capacity * RECIPIENT_KEY_BLOCKS,
    maxBlocks: RECIPIENT_KEY_BLOCKS,
    length: layout.vouched ? RECIPIENT_KEY_LEN : 0,
  });
}

/** `32 m`, the transmitted key region, then the voucher's key on a vouched frame. */
export function recipientKeysLen(layout: Pick<AuditStreamLayout, "outputs" | "vouched">): number {
  return RECIPIENT_KEY_LEN * (layout.outputs + (layout.vouched ? 1 : 0));
}

/** `0x04 ‖ eph_u ‖ envelope_ct ‖ keys_ct ‖ record_ct of each exit`. */
export function auditorMessageLen(layout: AuditStreamLayout, exits: number): number {
  return (
    TAGGED_KEY_LEN +
    envelopeRegion(layout).length +
    recipientKeysLen(layout) +
    publishedRecordLen(layout.records, RecordKind.exit) * exits
  );
}

/** The blocks a reader decrypts. */
export function capacityBlocks(layout: Pick<AuditStreamLayout, "key" | "capacity">): number {
  return envelopeBlocks(layout.key) + (layout.capacity + 1) * RECIPIENT_KEY_BLOCKS;
}

function shapeError(name: string): InterfaceError {
  return new InterfaceError("INTERFACE_INVALID_SHAPE", { name });
}
