import type { Bytes32 } from "@heliuslabs/spr/interface";

import {
  RECORD_ZERO_KEY_BIT,
  RecordKind,
  decodeOutputRecord,
  namedNullifierPk,
  publishedRecordLen,
  type OutputRecord,
} from "../../src/protocol/codec/audit-stream.js";
import {
  EXIT_RECORD_LEN,
  RING_RECORD_LEN,
} from "../../src/protocol/codec/ring-disclosure-layout.js";

/** A slot's published record and the key its commitment binds, absent on a dummy. */
export interface VectorRecord {
  readonly record: OutputRecord;
  readonly nullifierPk: Bytes32 | undefined;
}

/** `plaintext` is the full record the circuit encrypts, a non-exit slot publishes its first `RING_RECORD_LEN` bytes. */
export function vectorRecord(plaintext: Uint8Array): VectorRecord {
  const kind = ((plaintext[0] ?? 0) & ~RECORD_ZERO_KEY_BIT) as RecordKind;
  const record = decodeOutputRecord(plaintext.subarray(0, publishedRecordLen(kind)));
  const nullifierPk =
    namedNullifierPk(record.key) ??
    (record.kind === RecordKind.ring
      ? (plaintext.slice(RING_RECORD_LEN, EXIT_RECORD_LEN) as Bytes32)
      : undefined);
  return { record, nullifierPk };
}
