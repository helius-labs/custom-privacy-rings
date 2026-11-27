/**
 * The auditor stream of a transfer and the per-slot output record, sealed and opened over their
 * canonical plaintexts, Rust `AuditStreamSeal`, `AuditEnvelope` and `OutputRecord`.
 */
import { checkedBytes, clampViewingSecret, concatBytes } from "@heliuslabs/spr/internal";
import {
  ViewingKey,
  type Bytes16,
  type Bytes32,
  type X25519Keypair,
  type X25519PublicKey,
} from "@heliuslabs/spr/keypair";

import { RingError } from "../../protocol/error.js";
import {
  decodeAuditEnvelope,
  decodeOutputRecord,
  decodeRecipientPayload,
  encodeAuditEnvelope,
  encodeRecipientPayload,
  publishedOutputRecord,
  RecordKind,
  RECORD_ZERO_KEY_BIT,
  type AuditEnvelope,
  type OutputRecord,
  type RecipientPayload,
} from "../../protocol/codec/audit-stream.js";
import {
  ENVELOPE_KEY_LEN,
  RECIPIENT_KEY_LEN,
  STREAM_FIRST_COUNTER,
  checkedStreamLayout,
  envelopeRegion,
  publishedRecordLen,
  recipientKeyRegion,
  voucherRegion,
  type AuditStreamLayout,
  type RecordForm,
} from "../../protocol/codec/ring-disclosure-layout.js";
import { RingKeyError } from "../error.js";
import {
  OUTPUT_RECORD_ENC_INFO,
  saltedAuditorSchedule,
  sealedAuditorSchedule,
  sealSaltedAuditorStream,
  symmetricAuditApply,
  type AuditSchedule,
  type AuditorEnvelope,
  type AuditorMessage,
} from "./audit.js";
import { defined } from "../../protocol/defined.js";

/** One transfer's auditor plaintexts, Rust `AuditStreamSeal`. */
export interface RingDisclosure {
  readonly envelope: AuditEnvelope;
  /** `V_i` of each published slot, the key its record seals to. */
  readonly recipientKeys: readonly Bytes32[];
  /** The Ed25519 key whose vouch the proof verified, present exactly on a vouched layout. */
  readonly voucher?: Bytes32;
}

/** `layout` is the one the program verified. */
export interface PublishedAudit {
  readonly layout: AuditStreamLayout;
  readonly message: Pick<
    AuditorMessage,
    "ephemeralPublicKey" | "envelope" | "recipientKeys" | "voucher"
  >;
  readonly salt: Bytes16;
}

/** Rust `PayloadRead`, a malformed payload loses the application data and keeps the proven record. */
export type PayloadRead =
  /** A frame without data. */
  | Readonly<{ kind: "absent" }>
  | Readonly<{ kind: "opened"; payload: RecipientPayload }>
  | Readonly<{ kind: "malformed" }>;

export interface OpenedOutput {
  readonly record: OutputRecord;
  readonly payload: PayloadRead;
}

/** Refuses a disclosure off its layout before sealing. */
export function sealRingDisclosure(
  input: Readonly<{
    layout: AuditStreamLayout;
    auditorPublicKey: X25519PublicKey;
    salt: Bytes16;
    disclosure: RingDisclosure;
  }>,
): AuditorEnvelope {
  const { disclosure, auditorPublicKey } = input;
  const layout = checkedStreamLayout(input.layout);
  if (
    disclosure.recipientKeys.length !== layout.outputs ||
    (disclosure.voucher !== undefined) !== layout.vouched ||
    (disclosure.envelope.evidence.kind === "entry") !== layout.key.entry
  )
    throw shapeError("disclosure");
  const envelope = encodeAuditEnvelope(disclosure.envelope);
  try {
    if (envelope.length !== envelopeRegion(layout).length) throw shapeError("envelope");
    return sealSaltedAuditorStream({
      txViewingSecret: disclosure.envelope.txViewingSecret,
      salt: input.salt,
      auditorPublicKey,
      envelope,
      recipientKeys: disclosure.recipientKeys.map((key, slot) => ({
        counter: recipientKeyRegion(layout, slot).startCounter,
        bytes: key,
      })),
      ...(disclosure.voucher === undefined
        ? {}
        : {
            voucher: {
              counter: voucherRegion(layout).startCounter,
              bytes: checkedBytes<Bytes32>(disclosure.voucher, RECIPIENT_KEY_LEN, "voucher"),
            },
          }),
    });
  } finally {
    envelope.fill(0);
  }
}

/** Every plaintext under the layout the program verified, each read at its own counter. */
export function openRingDisclosure(
  auditor: X25519Keypair,
  published: PublishedAudit,
): RingDisclosure {
  const { message, salt } = published;
  return readDisclosure(
    saltedAuditorSchedule(auditor, message.ephemeralPublicKey, salt),
    published,
  );
}

/** Rust `AuditEnvelope::open_sealed`, the wrong auditor key decrypts noise. */
export function openSealedDisclosure(
  txViewingSecret: Bytes32,
  auditorPublicKey: X25519PublicKey,
  published: PublishedAudit,
): RingDisclosure {
  return readDisclosure(
    sealedAuditorSchedule(txViewingSecret, auditorPublicKey, published.salt),
    published,
  );
}

/** The record `form` publishes then the payload as one `CRING/rcp` stream under `CRDS_i`, Rust `RecipientBody::seal`. */
export function sealOutputSlot(
  slotSecret: Bytes32,
  output: Readonly<{
    record: OutputRecord;
    form: RecordForm;
    payload?: RecipientPayload;
    payloadLength?: number;
  }>,
): Uint8Array {
  const plaintext = concatBytes(
    publishedOutputRecord(output.record, output.form),
    ...(output.payload === undefined
      ? []
      : [encodeRecipientPayload(output.payload, output.payloadLength)]),
  );
  try {
    return symmetricAuditApply(slotSecret, recordPosition(), plaintext);
  } finally {
    plaintext.fill(0);
  }
}

/** A non-exit body under the frame's `form`, the bytes past its record are the payload, an exit kind is refused. */
export function openOutputSlot(
  slotSecret: Bytes32,
  body: Uint8Array,
  form: RecordForm,
): OpenedOutput {
  const opened = openSlotBody(slotSecret, body, form);
  if (opened.record.kind === RecordKind.exit) throw shapeError("outputBody");
  return opened;
}

/** An exit's record under the frame's `form`, refused at any other length or kind. */
export function openExitRecord(
  slotSecret: Bytes32,
  recordCiphertext: Uint8Array,
  form: RecordForm,
): OutputRecord {
  const { record, payload } = openSlotBody(slotSecret, recordCiphertext, form);
  if (record.kind !== RecordKind.exit || payload.kind !== "absent") throw shapeError("exitRecord");
  return record;
}

/** The cipher is unauthenticated, only the commitment a reader rebuilds proves ownership, Rust `open_recipient_body`. */
function openSlotBody(slotSecret: Bytes32, body: Uint8Array, form: RecordForm): OpenedOutput {
  if (body.length === 0) throw shapeError("outputBody");
  const plaintext = symmetricAuditApply(slotSecret, recordPosition(), body);
  try {
    const exit = ((plaintext[0] ?? 0) & ~RECORD_ZERO_KEY_BIT) === RecordKind.exit;
    const recordLength = publishedRecordLen(form, exit ? RecordKind.exit : RecordKind.ring);
    if (body.length < recordLength) throw shapeError("outputBody");
    return Object.freeze({
      record: decodeOutputRecord(plaintext.subarray(0, recordLength), form),
      payload:
        body.length > recordLength ? payloadRead(plaintext.subarray(recordLength)) : ABSENT_PAYLOAD,
    });
  } finally {
    plaintext.fill(0);
  }
}

/** Wipes `recovered`, the circuit seals the clamped encoding and an alias is refused. */
export function canonicalViewingKey(recovered: Bytes32): ViewingKey {
  const canonical = clampViewingSecret(recovered);
  try {
    let difference = 0;
    for (let index = 0; index < ENVELOPE_KEY_LEN; index++)
      difference |= (recovered[index] ?? 0) ^ (canonical[index] ?? 0);
    if (difference !== 0) throw new RingError("RING_AUDIT_NONCANONICAL_KEY");
    return ViewingKey.fromBytes(recovered);
  } finally {
    canonical.fill(0);
    recovered.fill(0);
  }
}

function readDisclosure(schedule: AuditSchedule, published: PublishedAudit): RingDisclosure {
  const { message } = published;
  const opened: Uint8Array[] = [];
  const plain = (counter: number, bytes: Uint8Array): Uint8Array => {
    const plaintext = schedule.apply(counter, bytes);
    opened.push(plaintext);
    return plaintext;
  };
  try {
    const layout = checkedStreamLayout(published.layout);
    if (
      message.recipientKeys.length !== layout.outputs ||
      (message.voucher?.length ?? 0) !== voucherRegion(layout).length ||
      message.recipientKeys.some((key) => key.length !== RECIPIENT_KEY_LEN)
    )
      throw shapeError("disclosure");
    const envelope = decodeAuditEnvelope(plain(STREAM_FIRST_COUNTER, message.envelope), layout);
    const recipientKeys = message.recipientKeys.map(
      (key, slot) => schedule.apply(recipientKeyRegion(layout, slot).startCounter, key) as Bytes32,
    );
    const voucher =
      message.voucher === undefined
        ? undefined
        : (schedule.apply(voucherRegion(layout).startCounter, message.voucher) as Bytes32);
    return Object.freeze({
      envelope,
      recipientKeys: Object.freeze(recipientKeys),
      ...defined({ voucher }),
    });
  } finally {
    schedule.destroy();
    opened.forEach((bytes) => bytes.fill(0));
  }
}

function payloadRead(plaintext: Uint8Array): PayloadRead {
  try {
    return Object.freeze({ kind: "opened", payload: decodeRecipientPayload(plaintext) });
  } catch {
    return MALFORMED_PAYLOAD;
  }
}

export const ABSENT_PAYLOAD: PayloadRead = Object.freeze({ kind: "absent" });
const MALFORMED_PAYLOAD: PayloadRead = Object.freeze({ kind: "malformed" });

function recordPosition() {
  return Object.freeze({ label: OUTPUT_RECORD_ENC_INFO, counter: STREAM_FIRST_COUNTER });
}

function shapeError(name: string): RingKeyError {
  return new RingKeyError("KEYPAIR_DISCLOSURE_SHAPE", { name });
}
