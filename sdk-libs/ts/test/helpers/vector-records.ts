import type { Bytes32 } from "@heliuslabs/spr/interface";

import {
  RecordKind,
  decodeOutputRecord,
  namedNullifierPk,
  type OutputRecord,
} from "../../src/protocol/codec/audit-stream.js";

/** A slot's published record and the key it names, absent on a dummy and a nonzero-key ring output. */
export interface VectorRecord {
  readonly record: OutputRecord;
  readonly nullifierPk: Bytes32 | undefined;
}

/** The published plaintext, `RING_RECORD_LEN` bytes off an exit and `EXIT_RECORD_LEN` on one. */
export function vectorRecord(plaintext: Uint8Array): VectorRecord {
  const record = decodeOutputRecord(plaintext);
  return {
    record,
    nullifierPk: record.kind === RecordKind.dummy ? undefined : namedNullifierPk(record.key),
  };
}
