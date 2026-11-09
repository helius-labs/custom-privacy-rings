/**
 * The auditor stream of a transfer and the per-slot output record, sealed and opened over their
 * canonical plaintexts, Rust `AuditStreamSeal`, `AuditEnvelope` and `OutputRecord`.
 * Read after `protocol/codec/audit-stream.ts`. Read before `operations/payments/output-rail.ts` and the audit readers.
 */
import { clampViewingSecret, concatBytes } from "@heliuslabs/spr/internal";
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
  encodeOutputRecord,
  encodeRecipientPayload,
  publishedRecordLen,
  RecordKind,
  type AuditEnvelope,
  type OutputRecord,
  type RecipientPayload,
} from "../../protocol/codec/audit-stream.js";
import {
  ENVELOPE_KEY_LEN,
  EXIT_RECORD_LEN,
  RECIPIENT_KEY_LEN,
  RING_RECORD_LEN,
  STREAM_FIRST_COUNTER,
  checkedStreamLayout,
  envelopeRegion,
  recipientKeyRegion,
  type AuditStreamLayout,
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

/** One transfer's auditor plaintexts, Rust `AuditStreamSeal`. */
export interface RingDisclosure {
  readonly envelope: AuditEnvelope;
  /** `V_i` of each published slot, the key its record seals to. */
  readonly recipientKeys: readonly Bytes32[];
}

/** `layout` is the one the program verified. */
export interface PublishedAudit {
  readonly layout: AuditStreamLayout;
  readonly message: Pick<AuditorMessage, "ephemeralPublicKey" | "envelope" | "recipientKeys">;
  readonly salt: Bytes16;
}

/** Rust `PayloadRead`, a malformed payload loses its data hashes and keeps the proven record. */
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

/** The published record then the payload as one `CRING/rcp` stream under `CRDS_i`, Rust `RecipientPayload`. */
export function sealOutputSlot(
  slotSecret: Bytes32,
  output: Readonly<{ record: OutputRecord; payload?: RecipientPayload; payloadLength?: number }>,
): Uint8Array {
  const record = encodeOutputRecord(output.record);
  const plaintext = concatBytes(
    record.subarray(0, publishedRecordLen(output.record.kind)),
    ...(output.payload === undefined
      ? []
      : [encodeRecipientPayload(output.payload, output.payloadLength)]),
  );
  try {
    return symmetricAuditApply(slotSecret, recordPosition(), plaintext);
  } finally {
    record.fill(0);
    plaintext.fill(0);
  }
}

/** A non-exit body, the bytes past its record are the payload of a data frame, an exit kind is refused. */
export function openOutputSlot(slotSecret: Bytes32, body: Uint8Array): OpenedOutput {
  if (body.length < RING_RECORD_LEN) throw shapeError("outputBody");
  const plaintext = symmetricAuditApply(slotSecret, recordPosition(), body);
  try {
    const record = decodeOutputRecord(plaintext.subarray(0, RING_RECORD_LEN));
    return Object.freeze({
      record,
      payload:
        body.length > RING_RECORD_LEN
          ? payloadRead(plaintext.subarray(RING_RECORD_LEN))
          : ABSENT_PAYLOAD,
    });
  } finally {
    plaintext.fill(0);
  }
}

/** An exit's full record, refused at any other length or kind. */
export function openExitRecord(slotSecret: Bytes32, recordCiphertext: Uint8Array): OutputRecord {
  if (recordCiphertext.length !== EXIT_RECORD_LEN) throw shapeError("exitRecord");
  const plaintext = symmetricAuditApply(slotSecret, recordPosition(), recordCiphertext);
  try {
    const record = decodeOutputRecord(plaintext);
    if (record.kind !== RecordKind.exit) throw shapeError("exitRecord");
    return record;
  } finally {
    plaintext.fill(0);
  }
}

/** The plaintext concatenation the circuit hashes, `envelope ‖ keys`. */
export function disclosurePlaintext(disclosure: RingDisclosure): Uint8Array {
  return concatBytes(encodeAuditEnvelope(disclosure.envelope), ...disclosure.recipientKeys);
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
      message.recipientKeys.some((key) => key.length !== RECIPIENT_KEY_LEN)
    )
      throw shapeError("disclosure");
    const envelope = decodeAuditEnvelope(plain(STREAM_FIRST_COUNTER, message.envelope), layout);
    const recipientKeys = message.recipientKeys.map(
      (key, slot) => schedule.apply(recipientKeyRegion(layout, slot).startCounter, key) as Bytes32,
    );
    return Object.freeze({ envelope, recipientKeys: Object.freeze(recipientKeys) });
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
