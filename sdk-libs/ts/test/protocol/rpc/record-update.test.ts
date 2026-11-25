import { readFileSync } from "node:fs";

import { createKeyPairSignerFromPrivateKeyBytes, getBase58Decoder, signBytes } from "@solana/kit";
import { describe, expect, it } from "vitest";

import type { Address, Bytes32, Bytes33 } from "@heliuslabs/spr/interface";
import { addressBytes, indexerScalars } from "@heliuslabs/spr/internal";
import { P256ReaderKey } from "../../../src/operations/management/reader-access.js";
import {
  AuthorityRecordUpdate,
  WalletRecordUpdate,
} from "../../../src/operations/management/record-updates.js";
import { listWrite } from "../../../src/operations/management/record-writes.js";
import {
  ListId,
  memberOfTag,
  ownerRecordKey,
  recordBit,
} from "../../../src/protocol/membership/policy.js";
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
  verifyRecordUpdate,
} from "../../../src/protocol/ring-rpc/record-update.js";
import { encodeSubmitRecordUpdateRequest } from "../../../src/protocol/ring-rpc/ring-rpc-codec.js";

const base58 = getBase58Decoder();
const decoder = new TextDecoder();

type RecordUpdateVector = { message: string; wire: string; updateId: string };

function filled(byte: number): Address {
  return base58.decode(new Uint8Array(32).fill(byte)) as Address;
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
    const signed = await new WalletRecordUpdate({
      genesisHash: new Uint8Array(32).fill(3) as Bytes32,
      ringProgramId: filled(1),
      registry: filled(2),
      wallet: wallet.address,
      validUntilSlot: 4242n,
    })
      .withLists(listWrite({ set: [ListId.ringViewing] }))
      .withWalletGrant(
        P256ReaderKey.fromBytes(
          Uint8Array.from(
            Buffer.from(
              "036b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296",
              "hex",
            ),
          ) as Bytes33,
        ),
        "granted",
      )
      .sign(wallet);
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
    const signed = await new WalletRecordUpdate({
      genesisHash: new Uint8Array(32).fill(3) as Bytes32,
      ringProgramId: filled(1),
      registry: filled(2),
      wallet: wallet.address,
      validUntilSlot: 4242n,
    })
      .withLists(listWrite({ set: [ListId.ringViewing] }))
      .awaitingListApproval([ListId.allow])
      .sign(wallet);
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

  it("clears both keys and propagates unregisteredOwners in AuthorityRecordUpdate", async () => {
    const authority = await createKeyPairSignerFromPrivateKeyBytes(new Uint8Array(32).fill(9));
    const target = {
      genesisHash: new Uint8Array(32).fill(3) as Bytes32,
      ringProgramId: filled(1),
      registry: filled(2),
      authority: authority.address,
      validUntilSlot: 4242n,
    };
    const ownerPkHash = memberOfTag(addressBytes(owner, "owner"));
    const expectedOwner = indexerScalars.hash(privateListKey(ownerPkHash, listSalt));
    const expectedRing = indexerScalars.hash(ringListKey(ownerPkHash, ringSalt));
    const lists = [ListId.privateAllow, ListId.privateBlock] as const;
    const expectedMask = recordBit(ListId.privateAllow) | recordBit(ListId.privateBlock);

    const both = await new AuthorityRecordUpdate(target)
      .withOwnerPrivateClear({ owner, salts: { listSalt, ringSalt }, lists })
      .sign(authority);
    verifyRecordUpdate(both);
    expect(both.unregisteredOwners).toEqual([owner]);
    expect(encodeSubmitRecordUpdateRequest(both).unregisteredOwners).toEqual([owner]);
    expect(both.update.entries).toEqual([
      { kind: "write", mintPolicy: null, member: expectedOwner, mask: expectedMask, value: 0n },
      { kind: "write", mintPolicy: null, member: expectedRing, mask: expectedMask, value: 0n },
    ]);

    const ringOnly = await new AuthorityRecordUpdate(target)
      .withOwnerPrivateClear({ owner, salts: { ringSalt }, lists: [ListId.privateFrozen] })
      .withWrite(ownerRecordKey(addressBytes(owner, "owner")), listWrite({ clear: [ListId.allow] }))
      .sign(authority);
    verifyRecordUpdate(ringOnly);
    expect(ringOnly.unregisteredOwners).toEqual([owner]);
    expect(ringOnly.update.entries).toHaveLength(2);

    const ownerOnly = await new AuthorityRecordUpdate(target)
      .withOwnerPrivateClear({ owner, salts: { listSalt }, lists: [ListId.privateAllow] })
      .sign(authority);
    verifyRecordUpdate(ownerOnly);
    expect(ownerOnly.unregisteredOwners).toBeUndefined();
    expect(ownerOnly.update.entries).toHaveLength(1);

    const repeated = await new AuthorityRecordUpdate(target)
      .withOwnerPrivateClear({
        owner,
        salts: { listSalt, ringSalt },
        lists: [ListId.privateAllow],
      })
      .withOwnerPrivateClear({
        owner,
        salts: { listSalt, ringSalt },
        lists: [ListId.privateBlock],
      })
      .sign(authority);
    expect(repeated.unregisteredOwners).toEqual([owner]);

    const base = new AuthorityRecordUpdate(target);
    expect(() =>
      base.withOwnerPrivateClear({ owner, salts: { listSalt, ringSalt }, lists: [] }),
    ).toThrowError(expect.objectContaining({ code: "RING_LIST_WRITE_INVALID" }));
    expect(() =>
      base.withOwnerPrivateClear({
        owner,
        salts: { listSalt, ringSalt },
        lists: [ListId.ringViewing],
      }),
    ).toThrowError(expect.objectContaining({ code: "RING_LIST_WRITER_UNAUTHORIZED" }));
    for (const nonPrivate of [
      ListId.allow,
      ListId.block,
      ListId.frozen,
      ListId.reader,
      ListId.approval,
    ]) {
      expect(() =>
        base.withOwnerPrivateClear({
          owner,
          salts: { listSalt, ringSalt },
          lists: [nonPrivate],
        }),
      ).toThrowError(
        expect.objectContaining({
          code: "RING_RECORD_UPDATE_INVALID",
          details: { reason: "NonPrivateList" },
        }),
      );
    }
    expect(() =>
      base.withOwnerPrivateClear({
        owner,
        salts: {},
        lists: [ListId.privateAllow],
      }),
    ).toThrowError(expect.objectContaining({ code: "RING_ENTRY_INVALID" }));
  });
});
