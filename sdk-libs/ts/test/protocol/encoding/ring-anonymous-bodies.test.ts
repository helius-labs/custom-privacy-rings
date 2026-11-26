import { bytesToHex, hexToBytes } from "@noble/hashes/utils.js";
import { beforeAll, describe, expect, it } from "vitest";

import replyVector from "../../../../../test-vectors/wallet/reply_metadata.json" with { type: "json" };
import { initializePoseidon } from "@heliuslabs/spr";
import { concatBytes, encodeBase58, ZERO_32, ZERO_NULLIFIER_PK } from "@heliuslabs/spr/internal";
import type { Bytes32 } from "@heliuslabs/spr/interface";
import { X25519Keypair } from "@heliuslabs/spr/keypair";
import { Data } from "@heliuslabs/spr/transaction";

import {
  ABSENT_PAYLOAD,
  openExitRecord,
  openOutputSlot,
  sealOutputSlot,
} from "../../../src/crypto/audit/ring-disclosure.js";
import {
  OUTPUT_RECORD_ENC_INFO,
  REPLY_ENC_INFO,
  symmetricAuditApply,
} from "../../../src/crypto/audit/audit.js";
import { x25519FromU } from "../../../src/crypto/keys/x25519.js";
import {
  MIN_REPLY_LEN,
  openReply,
  replyLength,
  sealReply,
  type ReplyMetadata,
} from "../../../src/protocol/codec/anonymous-delivery.js";
import {
  DUMMY_RECORD,
  EMPTY_PAYLOAD,
  RECORD_ZERO_KEY_BIT,
  RecordKind,
  UNPUBLISHED_RECORD_KEY,
  ZERO_RECORD_KEY,
  carriedRecordKey,
  decodeMergeRecord,
  decodeOutputRecord,
  encodeMergeRecord,
  encodeOutputRecord,
  encodeRecipientPayload,
  publishedOutputRecord,
  sealedRecordKey,
  type MergeOutputRecord,
  type OutputRecord,
} from "../../../src/protocol/codec/audit-stream.js";
import {
  CLEAR_RECORD_FORM,
  EXIT_RECORD_LEN,
  MERGE_RECORD_LEN,
  RING_RECORD_LEN,
  STREAM_FIRST_COUNTER,
  TRANSFER_RECORD_LEN,
  publishedRecordLen,
  recipientBodyForms,
  type RecordForm,
} from "../../../src/protocol/codec/ring-disclosure-layout.js";

const SECRET = new Uint8Array(32).fill(0x11) as Bytes32;
const OTHER_SECRET = new Uint8Array(32).fill(0x12) as Bytes32;

function filled(byte: number): Bytes32 {
  return new Uint8Array(32).fill(byte) as Bytes32;
}

function keystream(label: Uint8Array, length: number): Uint8Array {
  return symmetricAuditApply(
    SECRET,
    { label, counter: STREAM_FIRST_COUNTER },
    new Uint8Array(length),
  );
}

function sealedReplyBytes(plaintext: Uint8Array): Uint8Array {
  return symmetricAuditApply(
    SECRET,
    { label: REPLY_ENC_INFO, counter: STREAM_FIRST_COUNTER },
    plaintext,
  );
}

/** A ring output, its nonzero key unpublished. */
const RECORD: OutputRecord = {
  kind: RecordKind.ring,
  amount: 0x0102_0304_0506_0708n,
  ownerKey: filled(0x33),
  mint: encodeBase58(filled(0x22)),
  blinding: filled(0x11),
  key: UNPUBLISHED_RECORD_KEY,
  dataHash: ZERO_32,
  ringDataHash: ZERO_32,
};
const DATA_HASH = filled(0x0d);
const RING_DATA_HASH = filled(0x0e);
const CLEAR = CLEAR_RECORD_FORM;
const FORMS: readonly RecordForm[] = [false, true].flatMap((data) =>
  [false, true].map((ringData) => ({ data, ringData })),
);
/** `record` under the hashes `form` publishes for its kind, the proof holds every other one to zero. */
const hashed = (record: OutputRecord, form: RecordForm): OutputRecord => ({
  ...record,
  dataHash: form.data ? DATA_HASH : ZERO_32,
  ringDataHash: form.ringData && record.kind !== RecordKind.exit ? RING_DATA_HASH : ZERO_32,
});
const EXIT_KEY = filled(0x0a);
const EXIT: OutputRecord = {
  ...RECORD,
  kind: RecordKind.exit,
  key: { kind: "carried", nullifierPk: EXIT_KEY },
};
const ZERO_KEY: OutputRecord = { ...RECORD, key: ZERO_RECORD_KEY };

function reply(data = new Data()): ReplyMetadata {
  return {
    viewingKey: X25519Keypair.fromBytes(filled(9)).publicKey(),
    ownerKey: filled(0x44),
    nullifierPk: filled(0x55),
    data,
  };
}

beforeAll(initializePoseidon);

describe("output record codec", () => {
  it("lays the key last, the blinding at the ring record's end", () => {
    const exit = encodeOutputRecord(EXIT, CLEAR);
    expect(exit).toHaveLength(TRANSFER_RECORD_LEN);
    expect(exit[0]).toBe(RecordKind.exit);
    expect(exit.subarray(73, RING_RECORD_LEN)).toEqual(EXIT.blinding);
    expect(exit.subarray(RING_RECORD_LEN, EXIT_RECORD_LEN)).toEqual(EXIT_KEY);
    // A ring record seals no nullifier key.
    expect(encodeOutputRecord(RECORD, CLEAR).subarray(RING_RECORD_LEN)).toEqual(new Uint8Array(64));
    const zero = encodeOutputRecord(ZERO_KEY, CLEAR);
    expect(zero[0]).toBe(RecordKind.ring | RECORD_ZERO_KEY_BIT);
    expect(zero.subarray(RING_RECORD_LEN)).toEqual(new Uint8Array(64));
    expect(encodeOutputRecord({ ...EXIT, key: ZERO_RECORD_KEY }, CLEAR)[0]).toBe(
      RecordKind.exit | RECORD_ZERO_KEY_BIT,
    );
    expect(carriedRecordKey(ZERO_NULLIFIER_PK as Bytes32)).toEqual(ZERO_RECORD_KEY);
  });

  it("names a ring key by the bit alone and carries an exit's", () => {
    for (const record of [RECORD, ZERO_KEY, EXIT, { ...EXIT, key: ZERO_RECORD_KEY }])
      expect(decodeOutputRecord(publishedOutputRecord(record, CLEAR), CLEAR)).toEqual(record);
    expect(sealedRecordKey(RecordKind.ring, EXIT_KEY)).toEqual(UNPUBLISHED_RECORD_KEY);
    expect(sealedRecordKey(RecordKind.exit, EXIT_KEY)).toEqual(EXIT.key);
    expect(sealedRecordKey(RecordKind.ring, ZERO_NULLIFIER_PK as Bytes32)).toEqual(ZERO_RECORD_KEY);
  });

  it("lays an exit's key then its data hash and a ring record's two hashes past the head", () => {
    const both = { data: true, ringData: true };
    const exit = encodeOutputRecord(hashed(EXIT, both), both);
    expect(exit.subarray(RING_RECORD_LEN, EXIT_RECORD_LEN)).toEqual(EXIT_KEY);
    expect(exit.subarray(EXIT_RECORD_LEN)).toEqual(DATA_HASH);
    const ring = encodeOutputRecord(hashed(RECORD, both), both);
    expect(ring.subarray(RING_RECORD_LEN, EXIT_RECORD_LEN)).toEqual(DATA_HASH);
    expect(ring.subarray(EXIT_RECORD_LEN)).toEqual(RING_DATA_HASH);
    // A clear data bit leaves the first hash cell to the ring data hash.
    const ringData = { data: false, ringData: true };
    expect(
      publishedOutputRecord(hashed(RECORD, ringData), ringData).subarray(RING_RECORD_LEN),
    ).toEqual(RING_DATA_HASH);
  });

  it("encodes and parses every form at its one length per kind", () => {
    for (const form of FORMS) {
      const hashes = Number(form.data) + Number(form.ringData);
      expect(publishedRecordLen(form, RecordKind.ring)).toBe(RING_RECORD_LEN + 32 * hashes);
      expect(publishedRecordLen(form, RecordKind.dummy)).toBe(RING_RECORD_LEN + 32 * hashes);
      expect(publishedRecordLen(form, RecordKind.exit)).toBe(
        EXIT_RECORD_LEN + 32 * Number(form.data),
      );
      for (const record of [hashed(RECORD, form), hashed(EXIT, form), DUMMY_RECORD]) {
        const published = publishedOutputRecord(record, form);
        expect(published).toHaveLength(publishedRecordLen(form, record.kind));
        expect(decodeOutputRecord(published, form)).toEqual(record);
        // No other length of the frame reads the record.
        expect(() => decodeOutputRecord(published.subarray(0, -1), form)).toThrow();
        expect(() => decodeOutputRecord(Uint8Array.of(...published, 0), form)).toThrow();
      }
    }
    expect(encodeOutputRecord(DUMMY_RECORD, { data: true, ringData: true })).toEqual(
      new Uint8Array(TRANSFER_RECORD_LEN),
    );
  });

  it("refuses a dummy that publishes a hash", () => {
    const form = { data: true, ringData: false };
    const dummy = new Uint8Array(publishedRecordLen(form, RecordKind.dummy));
    expect(decodeOutputRecord(dummy, form)).toEqual(DUMMY_RECORD);
    dummy[RING_RECORD_LEN + 31] = 1;
    expect(() => decodeOutputRecord(dummy, form)).toThrow();
  });

  it("refuses a 105-byte exit record and a 137-byte non-exit record under the clear form", () => {
    expect(() =>
      decodeOutputRecord(encodeOutputRecord(EXIT, CLEAR).subarray(0, RING_RECORD_LEN), CLEAR),
    ).toThrow();
    expect(() =>
      decodeOutputRecord(encodeOutputRecord(RECORD, CLEAR).subarray(0, EXIT_RECORD_LEN), CLEAR),
    ).toThrow();
    expect(() => decodeOutputRecord(new Uint8Array(EXIT_RECORD_LEN), CLEAR)).toThrow();
  });

  it("refuses a kind off the three, a dummy with the bit and an exit whose bit disagrees", () => {
    const ring = publishedOutputRecord(RECORD, CLEAR);
    for (const kind of [3, RecordKind.dummy | RECORD_ZERO_KEY_BIT, 8]) {
      const edited = ring.slice();
      edited[0] = kind;
      expect(() => decodeOutputRecord(edited, CLEAR)).toThrow();
    }
    const flagged = publishedOutputRecord(EXIT, CLEAR);
    flagged[0] = RecordKind.exit | RECORD_ZERO_KEY_BIT;
    expect(() => decodeOutputRecord(flagged, CLEAR)).toThrow();
    const unflagged = publishedOutputRecord({ ...EXIT, key: ZERO_RECORD_KEY }, CLEAR);
    unflagged[0] = RecordKind.exit;
    expect(() => decodeOutputRecord(unflagged, CLEAR)).toThrow();
  });

  it("keeps the merge record's key and bit, the full form then the seed", () => {
    const { dataHash: _dataHash, ringDataHash: _ringDataHash, ...head } = RECORD;
    const records: readonly MergeOutputRecord[] = [
      { ...head, key: ZERO_RECORD_KEY },
      { ...head, key: EXIT.key },
    ];
    for (const record of records) {
      const merge = { record, seed: filled(0x66) };
      const encoded = encodeMergeRecord(merge);
      expect(encoded).toHaveLength(MERGE_RECORD_LEN);
      expect(decodeMergeRecord(encoded)).toEqual(merge);
    }
    const flipped = encodeMergeRecord({ record: records[0]!, seed: filled(0x66) });
    flipped[0] = RecordKind.ring;
    expect(() => decodeMergeRecord(flipped)).toThrow();
  });
});

describe("output slot body", () => {
  it("publishes a ring record without its key and an exit with it", () => {
    const sealed = sealOutputSlot(SECRET, { record: RECORD, form: CLEAR });
    expect(sealed).toHaveLength(RING_RECORD_LEN);
    expect(openOutputSlot(SECRET, sealed, CLEAR)).toEqual({
      record: RECORD,
      payload: ABSENT_PAYLOAD,
    });
    expect(() => openOutputSlot(OTHER_SECRET, sealed, CLEAR)).toThrow();
    const exit = sealOutputSlot(SECRET, { record: EXIT, form: CLEAR });
    expect(exit).toHaveLength(EXIT_RECORD_LEN);
    expect(openExitRecord(SECRET, exit, CLEAR)).toEqual(EXIT);
  });

  it("continues the payload at the record's end of one stream under every data form", () => {
    const payload = { data: Uint8Array.of(1, 2, 3) };
    for (const form of FORMS.filter(({ data }) => data)) {
      const record = hashed(RECORD, form);
      const recordLength = publishedRecordLen(form, RecordKind.ring);
      const sealed = sealOutputSlot(SECRET, { record, form, payload, payloadLength: 9 });
      expect(sealed).toHaveLength(recordLength + 9);
      expect(sealed.subarray(0, recordLength)).toEqual(sealOutputSlot(SECRET, { record, form }));
      expect(openOutputSlot(SECRET, sealed, form)).toEqual({
        record,
        payload: { kind: "opened", payload },
      });
    }
    expect(encodeRecipientPayload(EMPTY_PAYLOAD)).toEqual(Uint8Array.of(0, 0));
  });

  it("opens an exit record at the one length its form publishes", () => {
    const form = { data: true, ringData: true };
    const record = hashed(EXIT, form);
    const sealed = sealOutputSlot(SECRET, { record, form });
    expect(sealed).toHaveLength(EXIT_RECORD_LEN + 32);
    expect(openExitRecord(SECRET, sealed, form)).toEqual(record);
    expect(() => openExitRecord(SECRET, sealed, CLEAR)).toThrow();
    expect(() => openExitRecord(SECRET, sealed.subarray(0, EXIT_RECORD_LEN), form)).toThrow();
  });

  it("admits the forms a body length can hold and no other", () => {
    expect(recipientBodyForms(RING_RECORD_LEN)).toEqual([CLEAR]);
    expect(recipientBodyForms(RING_RECORD_LEN + 32)).toEqual([{ data: false, ringData: true }]);
    expect(recipientBodyForms(RING_RECORD_LEN + 32 + 2)).toEqual([{ data: true, ringData: false }]);
    expect(recipientBodyForms(RING_RECORD_LEN + 64 + 2)).toEqual([
      { data: true, ringData: false },
      { data: true, ringData: true },
    ]);
    expect(recipientBodyForms(RING_RECORD_LEN - 1)).toEqual([]);
    expect(recipientBodyForms(RING_RECORD_LEN + 1)).toEqual([]);
  });

  it("refuses a body shorter than a record, an exit in a ring body and a ring record as an exit, and keeps a record past a malformed payload", () => {
    const sealed = sealOutputSlot(SECRET, { record: RECORD, form: CLEAR });
    expect(() => openOutputSlot(SECRET, sealed.subarray(1), CLEAR)).toThrow();
    // A payload byte past a record reads a length it cannot hold, the record stands without it.
    expect(openOutputSlot(SECRET, Uint8Array.of(...sealed, 0), CLEAR)).toEqual({
      record: RECORD,
      payload: { kind: "malformed" },
    });
    expect(
      openOutputSlot(SECRET, sealOutputSlot(SECRET, { record: DUMMY_RECORD, form: CLEAR }), CLEAR),
    ).toEqual({ record: DUMMY_RECORD, payload: ABSENT_PAYLOAD });
    const exit = sealOutputSlot(SECRET, { record: EXIT, form: CLEAR });
    expect(() => openOutputSlot(SECRET, exit, CLEAR)).toThrow();
    expect(() => openExitRecord(SECRET, exit.subarray(0, RING_RECORD_LEN), CLEAR)).toThrow();
    const form = { data: true, ringData: false };
    expect(() =>
      openExitRecord(
        SECRET,
        sealOutputSlot(SECRET, {
          record: RECORD,
          form,
          payload: EMPTY_PAYLOAD,
          payloadLength: 32,
        }),
        form,
      ),
    ).toThrow();
  });

  it("refuses an amount past u64 at sealing", () => {
    expect(() =>
      sealOutputSlot(SECRET, { record: { ...RECORD, amount: 1n << 64n }, form: CLEAR }),
    ).toThrow();
  });
});

describe("reply metadata", () => {
  it("opens what it sealed, padded to the frame's one length", () => {
    const short = reply();
    const long = reply(new Data([{ kind: "ringData", bytes: new Uint8Array(40).fill(3) }]));
    const length = replyLength([short, long]);
    expect(length).toBeGreaterThan(MIN_REPLY_LEN);
    expect(replyLength([])).toBe(MIN_REPLY_LEN);
    for (const metadata of [short, long]) {
      const sealed = sealReply(SECRET, metadata, length);
      expect(sealed).toHaveLength(length);
      const opened = openReply(SECRET, sealed);
      expect(opened?.claimed.viewingKey.toBytes()).toEqual(metadata.viewingKey.toBytes());
      expect(opened?.claimed.ownerKey).toEqual(metadata.ownerKey);
      expect(opened?.claimed.nullifierPk).toEqual(metadata.nullifierPk);
      expect(opened?.claimed.data.records()).toEqual(metadata.data.records());
    }
  });

  it("refuses a length its data does not fit at sealing", () => {
    const metadata = reply(new Data([{ kind: "ringData", bytes: new Uint8Array(40) }]));
    expect(() => sealReply(SECRET, metadata, MIN_REPLY_LEN)).toThrow(
      expect.objectContaining({ code: "RING_REPLY_INVALID", details: { reason: "length" } }),
    );
  });

  it("reads undefined on nonzero padding", () => {
    const plaintext = symmetricAuditApply(
      SECRET,
      { label: REPLY_ENC_INFO, counter: STREAM_FIRST_COUNTER },
      sealReply(SECRET, reply(), MIN_REPLY_LEN + 8),
    );
    expect(openReply(SECRET, sealedReplyBytes(plaintext))).toBeDefined();
    plaintext[plaintext.length - 1] = 1;
    expect(openReply(SECRET, sealedReplyBytes(plaintext))).toBeUndefined();
  });

  it("reads undefined on a short reply, a key past the field and bytes that decode to no data", () => {
    const honest = symmetricAuditApply(
      SECRET,
      { label: REPLY_ENC_INFO, counter: STREAM_FIRST_COUNTER },
      sealReply(SECRET, reply(), MIN_REPLY_LEN),
    );
    expect(openReply(SECRET, new Uint8Array(MIN_REPLY_LEN - 1))).toBeUndefined();
    const pastField = honest.slice();
    pastField.fill(0xff, 0, 32);
    expect(openReply(SECRET, sealedReplyBytes(pastField))).toBeUndefined();
    const garbage = honest.slice();
    garbage[96] = 0xff;
    expect(openReply(SECRET, sealedReplyBytes(garbage))).toBeUndefined();
  });

  it("differs from a record over the same bytes under one secret", () => {
    const metadata = reply();
    const sealedReply = sealReply(SECRET, metadata, MIN_REPLY_LEN);
    const plaintext = symmetricAuditApply(
      SECRET,
      { label: REPLY_ENC_INFO, counter: STREAM_FIRST_COUNTER },
      sealedReply,
    );
    const asRecord = symmetricAuditApply(
      SECRET,
      { label: OUTPUT_RECORD_ENC_INFO, counter: STREAM_FIRST_COUNTER },
      concatBytes(plaintext, new Uint8Array(RING_RECORD_LEN - MIN_REPLY_LEN)),
    );
    expect(sealedReply).not.toEqual(asRecord.subarray(0, MIN_REPLY_LEN));
  });
});

describe("the shared reply vector", () => {
  const slotSecret = hexToBytes(replyVector.slotSecret) as Bytes32;

  it.each(replyVector.opens)(
    "opens and seals again $name",
    ({ name, ciphertext, viewingU, ownerKey, nullifierPk, replyLen }) => {
      const data =
        name === "a memo"
          ? new Data([{ kind: "memo", bytes: new Uint8Array(12).fill(0xab) }])
          : new Data();
      const sealed = hexToBytes(ciphertext);
      expect(sealed).toHaveLength(replyLen);
      const opened = openReply(slotSecret, sealed);
      expect(bytesToHex(opened?.claimed.viewingKey.u() ?? ZERO_32)).toBe(viewingU);
      expect(bytesToHex(opened?.claimed.ownerKey ?? ZERO_32)).toBe(ownerKey);
      expect(bytesToHex(opened?.claimed.nullifierPk ?? ZERO_32)).toBe(nullifierPk);
      expect(opened?.claimed.data.records()).toEqual(data.records());
      const metadata: ReplyMetadata = {
        viewingKey: x25519FromU(hexToBytes(viewingU) as Bytes32),
        ownerKey: hexToBytes(ownerKey) as Bytes32,
        nullifierPk: hexToBytes(nullifierPk) as Bytes32,
        data,
      };
      expect(bytesToHex(sealReply(slotSecret, metadata, replyLen))).toBe(ciphertext);
    },
  );

  it.each(replyVector.refused)("reads undefined on $name", ({ ciphertext }) => {
    expect(openReply(slotSecret, hexToBytes(ciphertext))).toBeUndefined();
  });
});

describe("delivery streams", () => {
  it("names one CTR label per region and each seal runs under its own", () => {
    expect(
      new Set(
        [OUTPUT_RECORD_ENC_INFO, REPLY_ENC_INFO].map((label) =>
          Buffer.from(label).toString("latin1"),
        ),
      ).size,
    ).toBe(2);
    const streams = [OUTPUT_RECORD_ENC_INFO, REPLY_ENC_INFO].map((label) =>
      Buffer.from(keystream(label, RING_RECORD_LEN)).toString("hex"),
    );
    expect(new Set(streams).size).toBe(2);
    expect(
      Buffer.from(sealOutputSlot(SECRET, { record: DUMMY_RECORD, form: CLEAR })).toString("hex"),
    ).toBe(streams[0]);
  });
});
