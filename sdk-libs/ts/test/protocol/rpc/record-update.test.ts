import { readFileSync } from "node:fs";

import { createKeyPairSignerFromPrivateKeyBytes, getBase58Decoder, signBytes } from "@solana/kit";
import { describe, expect, it } from "vitest";

import type { Address, Bytes32, Bytes33 } from "@heliuslabs/spr/interface";
import { addressBytes, indexerScalars } from "@heliuslabs/spr/internal";
import {
  P256ReaderKey,
  readerKeyMember,
} from "../../../src/operations/management/reader-access.js";
import { WalletRecordUpdate } from "../../../src/operations/management/record-updates.js";
import { listWrite } from "../../../src/operations/management/record-writes.js";
import { ListId, memberOfTag, recordBit } from "../../../src/protocol/membership/policy.js";
import {
  committedRingListKey,
  privateListKey,
  privateRevocationKeys,
  publicListKey,
  ringListKey,
} from "../../../src/protocol/membership/private-list.js";
import {
  recordUpdateMessage,
  recordUpdateStatementHash,
  signRecordUpdate,
  verifyRecordUpdate,
  type UnsignedRecordUpdate,
} from "../../../src/protocol/ring-rpc/record-update.js";
import type { RecordUpdateEntry } from "../../../src/protocol/ring-rpc/ring-rpc-types.js";
import { encodeSubmitRecordUpdateRequest } from "../../../src/protocol/ring-rpc/ring-rpc-codec.js";

const base58 = getBase58Decoder();
const decoder = new TextDecoder();

type RecordUpdateVector = { message: string; wire: string; updateId: string };

function filled(byte: number): Address {
  return base58.decode(new Uint8Array(32).fill(byte)) as Address;
}

/** The wallet's `ringViewing` write, then `entries`. */
function update(
  wallet: Address,
  entries: readonly RecordUpdateEntry[],
  awaitedLists = 0n,
): UnsignedRecordUpdate {
  const lists = new WalletRecordUpdate({
    genesisHash: new Uint8Array(32).fill(3) as Bytes32,
    ringProgramId: filled(1),
    registry: filled(2),
    wallet,
    validUntilSlot: 4242n,
  }).withLists(listWrite({ set: [ListId.ringViewing] }));
  return {
    registry: filled(2),
    update: {
      genesisHash: indexerScalars.hash(new Uint8Array(32).fill(3) as Bytes32),
      ring: filled(1),
      entries: [...lists.entries(), ...entries],
      validUntilSlot: 4242n,
      awaitedLists,
    },
  };
}

describe("submitRecordUpdate", () => {
  it("signs a wallet update byte for byte as the Rust vector and verifies it", async () => {
    const vector = JSON.parse(
      readFileSync(
        new URL("../../../../../test-vectors/rpc/record_update.json", import.meta.url),
        "utf8",
      ),
    ) as RecordUpdateVector;
    const wallet = await createKeyPairSignerFromPrivateKeyBytes(new Uint8Array(32).fill(7));
    const reader = P256ReaderKey.fromBytes(
      Uint8Array.from(
        Buffer.from("036b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296", "hex"),
      ) as Bytes33,
    );
    const signed = await signRecordUpdate(
      update(wallet.address, [
        {
          kind: "walletGrant",
          wallet: indexerScalars.hash(memberOfTag(addressBytes(wallet.address, "wallet"))),
          reader: indexerScalars.hash(readerKeyMember(reader)),
          access: "granted",
        },
      ]),
      wallet,
    );
    expect(decoder.decode(recordUpdateMessage(signed, signed.signer))).toBe(vector.message);
    expect(JSON.stringify(encodeSubmitRecordUpdateRequest(signed))).toBe(vector.wire);
    // The unsigned hint keeps the update id.
    const hinted = { ...signed, unregisteredOwners: [filled(9)] };
    expect(encodeSubmitRecordUpdateRequest(hinted)).toEqual({
      ...encodeSubmitRecordUpdateRequest(signed),
      unregisteredOwners: [filled(9)],
    });
    expect(verifyRecordUpdate(hinted)).toEqual(verifyRecordUpdate(signed));
    expect(() =>
      encodeSubmitRecordUpdateRequest({ ...signed, unregisteredOwners: ["0" as Address] }),
    ).toThrow();
    expect(verifyRecordUpdate(signed)).toEqual(
      indexerScalars.hashBytes(vector.updateId as Parameters<typeof indexerScalars.hashBytes>[0]),
    );
    // A wallet signs the text, a signature over the bare statement hash never verifies.
    const bare = {
      ...signed,
      signature: base58.decode(
        await signBytes(wallet.keyPair.privateKey, recordUpdateStatementHash(signed)),
      ) as typeof signed.signature,
    };
    expect(() => verifyRecordUpdate(bare)).toThrow();
    // The signature covers the slot bound, a later one names another statement.
    const extended = { ...signed, update: { ...signed.update, validUntilSlot: 4243n } };
    expect(() => verifyRecordUpdate(extended)).toThrow();
  });
});

describe("list approval", () => {
  it("signs an update pending list approval byte for byte as the Rust vector and binds its awaited lists", async () => {
    const vector = JSON.parse(
      readFileSync(
        new URL("../../../../../test-vectors/rpc/list_approval_update.json", import.meta.url),
        "utf8",
      ),
    ) as RecordUpdateVector;
    const wallet = await createKeyPairSignerFromPrivateKeyBytes(new Uint8Array(32).fill(7));
    const signed = await signRecordUpdate(
      update(wallet.address, [], recordBit(ListId.allow)),
      wallet,
    );
    expect(decoder.decode(recordUpdateMessage(signed, signed.signer))).toBe(vector.message);
    expect(JSON.stringify(encodeSubmitRecordUpdateRequest(signed))).toBe(vector.wire);
    expect(verifyRecordUpdate(signed)).toEqual(
      indexerScalars.hashBytes(vector.updateId as Parameters<typeof indexerScalars.hashBytes>[0]),
    );
    // A signature pending list approval never verifies as a plain update.
    const plain = { ...signed, update: { ...signed.update, awaitedLists: 0n } };
    expect(() => verifyRecordUpdate(plain)).toThrow();
  });
});

describe("owner private-list revocation", () => {
  const owner = filled(11);
  const listSalt = new Uint8Array(32).fill(0x12) as Bytes32;
  const ringSalt = new Uint8Array(32).fill(0x2b) as Bytes32;
  const zeroSalt = new Uint8Array(32) as Bytes32;

  it("derives every key the circuit reads and refuses missing or zero salts", () => {
    const ownerPkHash = memberOfTag(addressBytes(owner, "owner"));
    const expectedOwner = privateListKey(ownerPkHash, listSalt);
    const expectedRing = ringListKey(ownerPkHash, ringSalt);

    expect(privateRevocationKeys(ownerPkHash, { listSalt, ringSalt })).toEqual([
      { member: expectedOwner, kind: "owner" },
      { member: expectedRing, kind: "ring" },
    ]);
    expect(privateRevocationKeys(ownerPkHash, { listSalt })).toEqual([
      { member: expectedOwner, kind: "owner" },
    ]);
    expect(privateRevocationKeys(ownerPkHash, { ringSalt })).toEqual([
      { member: expectedRing, kind: "ring" },
    ]);

    expect(() => privateRevocationKeys(ownerPkHash, {})).toThrowError(
      expect.objectContaining({
        code: "RING_ENTRY_INVALID",
        details: { reason: "missingListSalt" },
      }),
    );
    for (const salts of [
      { listSalt: zeroSalt },
      { ringSalt: zeroSalt },
      { listSalt, ringSalt: zeroSalt },
      { listSalt: zeroSalt, ringSalt },
    ]) {
      expect(() => privateRevocationKeys(ownerPkHash, salts)).toThrowError(
        expect.objectContaining({
          code: "RING_ENTRY_INVALID",
          details: { reason: "zeroSalt" },
        }),
      );
    }
    for (const length of [0, 31, 33]) {
      const bad = new Uint8Array(length) as Bytes32;
      const invalidLength = expect.objectContaining({ code: "INTERFACE_INVALID_LENGTH" });
      expect(() => publicListKey(bad)).toThrowError(invalidLength);
      expect(() => privateListKey(bad, listSalt)).toThrowError(invalidLength);
      expect(() => privateListKey(ownerPkHash, bad)).toThrowError(invalidLength);
      expect(() => ringListKey(bad, ringSalt)).toThrowError(invalidLength);
      expect(() => ringListKey(ownerPkHash, bad)).toThrowError(invalidLength);
      expect(() =>
        committedRingListKey({ ownerPkHash: bad, ringSalt, ringListTag: ringSalt }),
      ).toThrowError(invalidLength);
      expect(() =>
        committedRingListKey({ ownerPkHash, ringSalt: bad, ringListTag: ringSalt }),
      ).toThrowError(invalidLength);
      expect(() => committedRingListKey({ ownerPkHash, ringSalt, ringListTag: bad })).toThrowError(
        invalidLength,
      );
      expect(() => privateRevocationKeys(bad, { listSalt })).toThrowError(invalidLength);
      expect(() => privateRevocationKeys(ownerPkHash, { listSalt: bad })).toThrowError(
        invalidLength,
      );
      expect(() => privateRevocationKeys(ownerPkHash, { ringSalt: bad })).toThrowError(
        invalidLength,
      );
    }
  });
});
