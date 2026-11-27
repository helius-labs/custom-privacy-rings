import { describe, expect, it } from "vitest";
import { hexToBytes, bytesToHex } from "@noble/hashes/utils.js";
import { address } from "@solana/kit";

import { initializePoseidon } from "@heliuslabs/spr";
import { addressBytes, hashBytes, solanaOwnerIdentity } from "@heliuslabs/spr/internal";
import { ownerHash, X25519Keypair } from "@heliuslabs/spr/keypair";
import type { Bytes16, Bytes32 } from "@heliuslabs/spr/interface";

import rangesJson from "../../../../../test-vectors/audit/ring-disclosure-ranges.json" with { type: "json" };
import vectorJson from "../../../../../test-vectors/audit/audit-stream.json" with { type: "json" };
import { parseAuditorMessage, sealSaltedAuditorStream } from "../../../src/crypto/audit/audit.js";
import {
  ABSENT_PAYLOAD,
  openOutputSlot,
  openRingDisclosure,
  sealOutputSlot,
  sealRingDisclosure,
  type RingDisclosure,
} from "../../../src/crypto/audit/ring-disclosure.js";
import { ringBlindingSeed, ringOutputBlinding } from "../../../src/crypto/derivation/blinding.js";
import {
  decodeAuditEnvelope,
  decodeMergeRecord,
  decodeOutputRecord,
  decodeRecipientPayload,
  DUMMY_RECORD,
  encodeAuditEnvelope,
  encodeMergeRecord,
  encodeOutputRecord,
  encodeRecipientPayload,
  publishedOutputRecord,
  RecordKind,
  UNPUBLISHED_RECORD_KEY,
  type MergeOutputRecord,
  type OutputRecord,
} from "../../../src/protocol/codec/audit-stream.js";
import {
  auditLayoutValue,
  auditorMessageLen,
  capacityBlocks,
  checkedStreamLayout,
  CLEAR_RECORD_FORM,
  TRANSFER_RECORD_LEN,
  envelopeBlocks,
  envelopeRegion,
  EXIT_RECORD_LEN,
  firstRecipientKeyCounter,
  recipientKeyRegion,
  recipientKeysLen,
  splitSealedKeys,
  voucherRegion,
  RECIPIENT_KEY_LEN,
  RING_RECORD_LEN,
  type AuditStreamLayout,
} from "../../../src/protocol/codec/ring-disclosure-layout.js";
import { vectorLayout, vectorRecord } from "../../helpers/vector-records.js";
import { envelopeDigest, recordChain } from "../../../src/protocol/proof/statement.js";
import { dummyUtxoHash, fieldUtxoHash } from "../../../src/protocol/membership/utxo-hash.js";
import { RingError } from "../../../src/protocol/error.js";

/** The regenerated writers' field names, Rust `AuditStreamLayout::{envelope, recipient_key, keys_len}`. */
interface RangeKey {
  readonly class: number;
  readonly entry: boolean;
  readonly outputs: number;
  readonly envelope_blocks: number;
  readonly first_key_counter: number;
  readonly capacity_blocks: number;
  readonly envelope_len_by_input_count: readonly number[];
  readonly key_start_counters: readonly number[];
}

interface StreamCase {
  readonly name: string;
  readonly layout: number;
  readonly input_class: number;
  readonly outputs: number;
  readonly output_count: number;
  readonly input_count: number;
  readonly envelope_plaintext: string;
  readonly envelope_ciphertext: string;
  readonly recipient_keys: readonly string[];
  readonly recipient_keys_ciphertext: string;
  readonly voucher: string;
  readonly records: readonly string[];
  readonly record_ciphertexts: readonly string[];
  readonly output_hashes: readonly string[];
  readonly blindings: readonly string[];
  readonly envelope_digest: string;
  readonly record_chain: string;
}

const ranges = rangesJson as unknown as { readonly keys: readonly RangeKey[] };
const vector = vectorJson as unknown as {
  readonly auditor_sk: string;
  readonly tx_viewing_sk: string;
  readonly salt: string;
  readonly first_nullifier: string;
  readonly output_tree_id: number;
  readonly ring_id: string;
  readonly cases: readonly StreamCase[];
};

describe("the stream regions", () => {
  it("are the shared vector for every key", () => {
    for (const key of ranges.keys) {
      const layoutAt = (inputs: number): AuditStreamLayout => ({
        key: { class: key.class, entry: key.entry },
        inputs,
        outputs: key.outputs,
        capacity: key.outputs,
        vouched: false,
        records: CLEAR_RECORD_FORM,
      });
      const layout = layoutAt(1);
      expect(envelopeBlocks(layout.key)).toBe(key.envelope_blocks);
      expect(firstRecipientKeyCounter(layout.key)).toBe(key.first_key_counter);
      expect(capacityBlocks(layout)).toBe(key.capacity_blocks);
      key.envelope_len_by_input_count.forEach((length, index) =>
        expect(envelopeRegion(layoutAt(index + 1)).length).toBe(length),
      );
      key.key_start_counters.forEach((counter, slot) => {
        const region = recipientKeyRegion(layout, slot);
        expect(region.startCounter).toBe(counter);
        expect(region.length).toBe(32);
      });
    }
  });

  it("counts two blocks per key and transmits 32 bytes per published slot", () => {
    const layout: AuditStreamLayout = {
      key: { class: 2, entry: false },
      inputs: 2,
      outputs: 2,
      capacity: 4,
      vouched: false,
      records: CLEAR_RECORD_FORM,
    };
    expect(envelopeBlocks(layout.key)).toBe(3);
    expect(firstRecipientKeyCounter(layout.key)).toBe(5);
    expect([0, 1, 2, 3].map((slot) => recipientKeyRegion(layout, slot).startCounter)).toEqual([
      5, 7, 9, 11,
    ]);
    expect(recipientKeyRegion(layout, 2).length).toBe(0);
    expect(capacityBlocks(layout)).toBe(3 + 5 * 2);
    // `0x04 ‖ eph_u ‖ envelope ‖ keys ‖ one exit record`.
    expect(auditorMessageLen(layout, 1)).toBe(33 + 33 + 64 + 137);
    // The data bit alone widens an exit record.
    const records = { data: true, ringData: true };
    expect(auditorMessageLen({ ...layout, records }, 1)).toBe(33 + 33 + 64 + 169);
  });

  it("keeps the voucher's key past the last slot of the capacity and transmits it on a vouched frame alone", () => {
    const plain: AuditStreamLayout = {
      key: { class: 2, entry: false },
      inputs: 2,
      outputs: 2,
      capacity: 4,
      vouched: false,
      records: CLEAR_RECORD_FORM,
    };
    const vouched: AuditStreamLayout = { ...plain, vouched: true };
    expect(voucherRegion(plain)).toEqual({ startCounter: 13, maxBlocks: 2, length: 0 });
    expect(voucherRegion(vouched)).toEqual({ startCounter: 13, maxBlocks: 2, length: 32 });
    expect([0, 1, 2, 3].map((slot) => recipientKeyRegion(vouched, slot).startCounter)).toEqual([
      5, 7, 9, 11,
    ]);
    expect(recipientKeysLen(plain)).toBe(64);
    expect(recipientKeysLen(vouched)).toBe(96);
    expect(auditorMessageLen(vouched, 1)).toBe(auditorMessageLen(plain, 1) + 32);
    expect(capacityBlocks(vouched)).toBe(capacityBlocks(plain));
    expect(
      [
        { entry: false, vouched: false, records: CLEAR_RECORD_FORM },
        { entry: false, vouched: true, records: CLEAR_RECORD_FORM },
        { entry: true, vouched: false, records: CLEAR_RECORD_FORM },
        { entry: true, vouched: true, records: CLEAR_RECORD_FORM },
      ].map(auditLayoutValue),
    ).toEqual([0, 1, 2, 3]);
    const clear = { entry: false, vouched: false };
    expect(auditLayoutValue({ ...clear, records: { data: true, ringData: false } })).toBe(4);
    expect(auditLayoutValue({ ...clear, records: { data: false, ringData: true } })).toBe(8);
    expect([0, 5, 10, 15].map((value) => auditLayoutValue(vectorLayout(value)))).toEqual([
      0, 5, 10, 15,
    ]);
    const sealed = Uint8Array.from({ length: 96 }, (_, at) => at);
    expect(splitSealedKeys(plain, sealed)).toEqual({ keys: sealed });
    expect(splitSealedKeys(vouched, sealed)).toEqual({
      keys: sealed.subarray(0, 64),
      voucher: sealed.subarray(64),
    });
    expect(() => splitSealedKeys(vouched, new Uint8Array(31))).toThrow();
  });

  it("refuses an input past the class and an output past the capacity", () => {
    const base: AuditStreamLayout = {
      key: { class: 2, entry: false },
      inputs: 2,
      outputs: 2,
      capacity: 2,
      vouched: false,
      records: CLEAR_RECORD_FORM,
    };
    expect(checkedStreamLayout(base)).toBe(base);
    expect(() => checkedStreamLayout({ ...base, inputs: 3 })).toThrow();
    expect(() => checkedStreamLayout({ ...base, inputs: 0 })).toThrow();
    expect(() => checkedStreamLayout({ ...base, outputs: 3 })).toThrow();
  });
});

const keyCiphertexts = (testCase: StreamCase): string[] =>
  testCase.recipient_keys.map((_, slot) =>
    testCase.recipient_keys_ciphertext.slice(
      slot * 2 * RECIPIENT_KEY_LEN,
      (slot + 1) * 2 * RECIPIENT_KEY_LEN,
    ),
  );

const voucherCiphertext = (testCase: StreamCase): string =>
  testCase.recipient_keys_ciphertext.slice(testCase.recipient_keys.length * 2 * RECIPIENT_KEY_LEN);

describe("the stream vector", () => {
  const auditor = (): X25519Keypair =>
    X25519Keypair.fromBytes(hexToBytes(vector.auditor_sk) as Bytes32);

  for (const testCase of vector.cases) {
    const { entry, vouched, records } = vectorLayout(testCase.layout);
    const layout: AuditStreamLayout = {
      key: { class: testCase.input_class, entry },
      inputs: testCase.input_count,
      outputs: testCase.output_count,
      capacity: testCase.outputs,
      vouched,
      records,
    };
    const salt = hexToBytes(vector.salt) as Bytes16;
    const txKey = (): X25519Keypair =>
      X25519Keypair.fromBytes(hexToBytes(vector.tx_viewing_sk) as Bytes32);
    const message = () => ({
      ephemeralPublicKey: txKey().publicKey(),
      envelope: hexToBytes(testCase.envelope_ciphertext),
      recipientKeys: keyCiphertexts(testCase).map(hexToBytes),
      ...(testCase.voucher === "" ? {} : { voucher: hexToBytes(voucherCiphertext(testCase)) }),
    });

    it(`${testCase.name} seals byte for byte as Rust and refuses another salt`, async () => {
      await initializePoseidon();
      const sealed = sealSaltedAuditorStream({
        txViewingSecret: hexToBytes(vector.tx_viewing_sk) as Bytes32,
        salt,
        auditorPublicKey: auditor().publicKey(),
        envelope: hexToBytes(testCase.envelope_plaintext),
        recipientKeys: testCase.recipient_keys.map((key, slot) => ({
          counter: recipientKeyRegion(layout, slot).startCounter,
          bytes: hexToBytes(key),
        })),
        ...(testCase.voucher === ""
          ? {}
          : {
              voucher: {
                counter: voucherRegion(layout).startCounter,
                bytes: hexToBytes(testCase.voucher),
              },
            }),
      });
      expect(bytesToHex(sealed.voucher ?? new Uint8Array())).toBe(voucherCiphertext(testCase));
      expect(bytesToHex(sealed.envelope)).toBe(testCase.envelope_ciphertext);
      expect(sealed.recipientKeys.map((bytes) => bytesToHex(bytes))).toEqual(
        keyCiphertexts(testCase),
      );
      const other = Uint8Array.from(salt, (byte) => byte ^ 1) as Bytes16;
      expect(() =>
        openRingDisclosure(auditor(), { layout, message: message(), salt: other }),
      ).toThrow(RingError);
    });

    it(`${testCase.name} opens under the auditor to its plaintexts`, async () => {
      await initializePoseidon();
      const opened = openRingDisclosure(auditor(), { layout, message: message(), salt });
      expect(bytesToHex(encodeAuditEnvelope(opened.envelope))).toBe(testCase.envelope_plaintext);
      expect(opened.recipientKeys.map((key) => bytesToHex(key))).toEqual(testCase.recipient_keys);
      expect(bytesToHex(opened.voucher ?? new Uint8Array())).toBe(testCase.voucher);
    });

    it(`${testCase.name} binds its digest and record chain`, async () => {
      await initializePoseidon();
      const outputs = testCase.record_ciphertexts.map((record, slot) => ({
        record: hexToBytes(record),
        exit:
          vectorRecord(hexToBytes(testCase.records[slot] as string), records).record.kind ===
          RecordKind.exit,
      }));
      expect(
        bytesToHex(
          envelopeDigest(
            hexToBytes(testCase.envelope_ciphertext),
            hexToBytes(testCase.recipient_keys_ciphertext),
            layout,
          ),
        ),
      ).toBe(testCase.envelope_digest);
      expect(bytesToHex(recordChain(outputs, testCase.outputs, records))).toBe(
        testCase.record_chain,
      );
    });

    it(`${testCase.name} rebuilds every published hash from its record alone`, async () => {
      await initializePoseidon();
      const opened = openRingDisclosure(auditor(), { layout, message: message(), salt });
      const firstNullifier = hexToBytes(vector.first_nullifier) as Bytes32;
      const seed = ringBlindingSeed(hexToBytes(vector.tx_viewing_sk) as Bytes32, salt);
      testCase.records.forEach((plaintext, slot) => {
        const { record, nullifierPk } = vectorRecord(hexToBytes(plaintext), records);
        const blinding = ringOutputBlinding({
          firstNullifier,
          blindingSeed: seed,
          outputIndex: slot,
        });
        expect(bytesToHex(blinding)).toBe(testCase.blindings[slot]);
        if (record.kind !== RecordKind.dummy) expect(record.blinding).toEqual(blinding);
        // A nonzero-key ring record names no key, the auditor's tracking read supplies it.
        if (record.kind === RecordKind.ring && nullifierPk === undefined) return;
        const hash =
          record.kind === RecordKind.dummy
            ? dummyUtxoHash(vector.output_tree_id, blinding)
            : fieldUtxoHash({
                owner: ownerHash(solanaOwnerIdentity(record.ownerKey), nullifierPk!) as Bytes32,
                assetField: hashBytes(addressBytes(record.mint, "mint")) as Bytes32,
                amount: record.amount,
                blinding: record.blinding,
                treeId: vector.output_tree_id,
                dataHash: record.dataHash,
                ringDataHash: record.ringDataHash,
                ringId:
                  record.kind === RecordKind.ring
                    ? (hexToBytes(vector.ring_id) as Bytes32)
                    : (new Uint8Array(32) as Bytes32),
              });
        expect(bytesToHex(hash)).toBe(testCase.output_hashes[slot]);
      });
      expect(opened.recipientKeys).toHaveLength(testCase.output_count);
    });
  }

  it("seals what it opens and reads a key only at its own counter", async () => {
    await initializePoseidon();
    const layout: AuditStreamLayout = {
      key: { class: 2, entry: false },
      inputs: 1,
      outputs: 2,
      capacity: 2,
      vouched: false,
      records: CLEAR_RECORD_FORM,
    };
    const keys = [
      X25519Keypair.generate().publicKey().u(),
      X25519Keypair.generate().publicKey().u(),
    ];
    const disclosure: RingDisclosure = {
      envelope: {
        txViewingSecret: hexToBytes(vector.tx_viewing_sk) as Bytes32,
        realInputs: 1,
        evidence: { kind: "none" },
      },
      recipientKeys: keys,
    };
    const keypair = X25519Keypair.fromBytes(hexToBytes(vector.auditor_sk) as Bytes32);
    const salt = hexToBytes(vector.salt) as Bytes16;
    const sealed = sealRingDisclosure({
      layout,
      auditorPublicKey: keypair.publicKey(),
      salt,
      disclosure,
    });
    const exitRecord = new Uint8Array(137).fill(5);
    const message = parseAuditorMessage(
      Uint8Array.of(
        ...sealed.ephemeralPublicKey.toBytes(),
        ...sealed.envelope,
        ...sealed.recipientKeys.flatMap((bytes) => [...bytes]),
        ...exitRecord,
      ),
      layout,
      1,
    );
    expect(message.exitRecords).toEqual([exitRecord]);
    const opened = openRingDisclosure(keypair, { layout, message, salt });
    expect(opened.recipientKeys).toEqual(keys);
    const [first, second] = message.recipientKeys;
    const crossed = { ...message, recipientKeys: [second as Uint8Array, first as Uint8Array] };
    expect(
      openRingDisclosure(keypair, { layout, message: crossed, salt }).recipientKeys,
    ).not.toEqual(keys);
    expect(() =>
      sealRingDisclosure({
        layout: { ...layout, outputs: 1 },
        auditorPublicKey: keypair.publicKey(),
        salt,
        disclosure,
      }),
    ).toThrow();
    expect(() => parseAuditorMessage(Uint8Array.of(...sealed.envelope), layout, 0)).toThrow();
  });
});

describe("the stream codecs", () => {
  const nullifierPk = new Uint8Array(32).fill(2) as Bytes32;
  /** A merge output's record, the full form carries its key. */
  const mergeRecord: MergeOutputRecord = {
    kind: RecordKind.ring,
    amount: 7n,
    ownerKey: new Uint8Array(32).fill(1) as Bytes32,
    mint: address("11111111111111111111111111111111"),
    blinding: new Uint8Array(32).fill(3) as Bytes32,
    key: { kind: "carried", nullifierPk },
  };
  /** The same note as a transfer's ring output, its key unpublished. */
  const record: OutputRecord = {
    ...mergeRecord,
    key: UNPUBLISHED_RECORD_KEY,
    dataHash: new Uint8Array(32).fill(4) as Bytes32,
    ringDataHash: new Uint8Array(32).fill(5) as Bytes32,
  };
  const form = { data: true, ringData: true };

  it("lays the output record out at its contract bytes", () => {
    const bytes = encodeOutputRecord(record, form);
    expect(bytes).toHaveLength(TRANSFER_RECORD_LEN);
    expect(bytes[0]).toBe(1);
    expect(bytes[8]).toBe(7);
    expect(bytes.subarray(73, RING_RECORD_LEN)).toEqual(record.blinding);
    expect(bytes.subarray(RING_RECORD_LEN, EXIT_RECORD_LEN)).toEqual(record.dataHash);
    expect(bytes.subarray(EXIT_RECORD_LEN)).toEqual(record.ringDataHash);
    expect(decodeOutputRecord(bytes, form)).toEqual(record);
    expect(encodeOutputRecord(DUMMY_RECORD, form).every((byte) => byte === 0)).toBe(true);
  });

  it("refuses a kind off the three, a dummy with any nonzero byte and another length", () => {
    const bytes = publishedOutputRecord(record, form);
    const kind = bytes.slice();
    kind[0] = 3;
    const dummy = bytes.slice();
    dummy[0] = 0;
    const dummyBlinding = new Uint8Array(TRANSFER_RECORD_LEN);
    dummyBlinding[RING_RECORD_LEN - 1] = 1;
    for (const refused of [kind, dummy, dummyBlinding, bytes.subarray(1)])
      expect(() => decodeOutputRecord(refused, form)).toThrow(RingError);
  });

  it("carries a 169 byte merge record of a ring note", () => {
    const seed = new Uint8Array(32).fill(5) as Bytes32;
    const merge = encodeMergeRecord({ record: mergeRecord, seed });
    expect(merge).toHaveLength(169);
    expect(merge.subarray(RING_RECORD_LEN, EXIT_RECORD_LEN)).toEqual(nullifierPk);
    expect(decodeMergeRecord(merge)).toEqual({ record: mergeRecord, seed });
    // The codec reads any kind as Rust does, the merge reader refuses one other than a ring note.
    const exit = merge.slice();
    exit[0] = RecordKind.exit;
    expect(decodeMergeRecord(exit).record.kind).toBe(RecordKind.exit);
    expect(() => decodeMergeRecord(merge.subarray(1))).toThrow(RingError);
  });

  it("seals the voucher's key at its own counter and opens it on a vouched layout alone", async () => {
    await initializePoseidon();
    const layout: AuditStreamLayout = {
      key: { class: 2, entry: false },
      inputs: 1,
      outputs: 2,
      capacity: 4,
      vouched: true,
      records: CLEAR_RECORD_FORM,
    };
    const keys = [
      X25519Keypair.generate().publicKey().u(),
      X25519Keypair.generate().publicKey().u(),
    ];
    const voucher = new Uint8Array(32).fill(0x76) as Bytes32;
    const disclosure: RingDisclosure = {
      envelope: {
        txViewingSecret: hexToBytes(vector.tx_viewing_sk) as Bytes32,
        realInputs: 1,
        evidence: { kind: "none" },
      },
      recipientKeys: keys,
      voucher,
    };
    const keypair = X25519Keypair.fromBytes(hexToBytes(vector.auditor_sk) as Bytes32);
    const salt = hexToBytes(vector.salt) as Bytes16;
    const seal = { auditorPublicKey: keypair.publicKey(), salt };
    const sealed = sealRingDisclosure({ ...seal, layout, disclosure });
    expect(sealed.voucher).toHaveLength(32);
    expect(sealed.voucher).not.toEqual(voucher);
    const exitRecord = new Uint8Array(137).fill(5);
    const data = Uint8Array.of(
      ...sealed.ephemeralPublicKey.toBytes(),
      ...sealed.envelope,
      ...sealed.recipientKeys.flatMap((bytes) => [...bytes]),
      ...(sealed.voucher ?? []),
      ...exitRecord,
    );
    const message = parseAuditorMessage(data, layout, 1);
    expect(message.voucher).toEqual(sealed.voucher);
    expect(message.exitRecords).toEqual([exitRecord]);
    const opened = openRingDisclosure(keypair, { layout, message, salt });
    expect(opened.voucher).toEqual(voucher);
    expect(opened.recipientKeys).toEqual(keys);
    // The voucher region starts at the capacity, a narrower key reads another counter.
    const narrow = { ...layout, capacity: 2 };
    expect(openRingDisclosure(keypair, { layout: narrow, message, salt }).voucher).not.toEqual(
      voucher,
    );
    const plain = { ...layout, vouched: false };
    expect(() => parseAuditorMessage(data, plain, 1)).toThrow();
    expect(() => openRingDisclosure(keypair, { layout: plain, message, salt })).toThrow();
    expect(() => sealRingDisclosure({ ...seal, layout: plain, disclosure })).toThrow();
    expect(() =>
      sealRingDisclosure({ ...seal, layout, disclosure: { ...disclosure, recipientKeys: keys } }),
    ).not.toThrow();
    const { voucher: _dropped, ...unvouched } = disclosure;
    expect(() => sealRingDisclosure({ ...seal, layout, disclosure: unvouched })).toThrow();
    expect(sealRingDisclosure({ ...seal, layout: plain, disclosure: unvouched }).voucher).toBe(
      undefined,
    );
  });

  it("refuses an envelope off its layout", () => {
    const layout: AuditStreamLayout = {
      key: { class: 2, entry: true },
      inputs: 1,
      outputs: 2,
      capacity: 2,
      vouched: false,
      records: CLEAR_RECORD_FORM,
    };
    const envelope = new Uint8Array(envelopeRegion(layout).length);
    expect(decodeAuditEnvelope(envelope, layout).evidence.kind).toBe("entry");
    expect(() => decodeAuditEnvelope(envelope.subarray(1), layout)).toThrow(RingError);
  });

  it("frames the payload with its length and refuses what the encoder never writes", () => {
    expect(encodeRecipientPayload({ data: new Uint8Array() })).toEqual(Uint8Array.of(0, 0));
    const data = new Uint8Array(40).fill(6);
    const padded = encodeRecipientPayload({ data }, 50);
    expect(padded).toHaveLength(50);
    expect([...padded.subarray(0, 2)]).toEqual([0, 40]);
    expect(decodeRecipientPayload(padded)).toEqual({ data });
    expect(decodeRecipientPayload(Uint8Array.of(0, 0, 0))).toEqual({ data: new Uint8Array() });
    const padding = padded.slice();
    padding[49] = 1;
    const pastBytes = Uint8Array.of(0, 9, 0);
    for (const refused of [padding, pastBytes, new Uint8Array(1)])
      expect(() => decodeRecipientPayload(refused)).toThrow(RingError);
    expect(() => encodeRecipientPayload({ data }, 10)).toThrow(RingError);
  });

  it("seals the record and payload as one stream the slot secret opens", async () => {
    await initializePoseidon();
    const secret = new Uint8Array(32).fill(9) as Bytes32;
    secret[0] = 0;
    const payload = { data: new Uint8Array(40).fill(6) };
    const body = sealOutputSlot(secret, { record, form, payload, payloadLength: 50 });
    // A ring slot publishes the record with its two hashes, the payload follows at its end.
    expect(body).toHaveLength(TRANSFER_RECORD_LEN + 50);
    expect(openOutputSlot(secret, body, form)).toEqual({
      record,
      payload: { kind: "opened", payload },
    });
    const bare = sealOutputSlot(secret, { record, form });
    // One keystream from counter 2, the record bytes seal alike with or without a payload.
    expect(bare).toEqual(body.subarray(0, TRANSFER_RECORD_LEN));
    expect(openOutputSlot(secret, bare, form)).toEqual({ record, payload: ABSENT_PAYLOAD });
    expect(() => openOutputSlot(secret, bare.subarray(1), form)).toThrow();
  });
});
