import { readFileSync } from "node:fs";

import { createKeyPairSignerFromPrivateKeyBytes, getBase58Decoder, signBytes } from "@solana/kit";
import { describe, expect, it } from "vitest";

import type { Address, Bytes32, Bytes33 } from "@heliuslabs/spr/interface";
import { addressBytes, indexerScalars } from "@heliuslabs/spr/internal";
import { P256ReaderKey, readerKeyMember } from "../../../src/protocol/membership/reader-access.js";
import { WalletRecordUpdate } from "../../../src/operations/management/record-updates.js";
import { listWrite } from "../../../src/protocol/membership/record-writes.js";
import { listTagOf } from "../../../src/crypto/keys/list-salt.js";
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
import { EMPTY_MINT_POLICY } from "../../../src/protocol/membership/mint-policy.js";
import { RingRpc } from "../../../src/rpc/ring-rpc.js";

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

describe("exact u64", () => {
  it("sends a mint policy cap and limit past 2^53 in their exact digits", async () => {
    const u64Max = (1n << 64n) - 1n;
    const wallet = await createKeyPairSignerFromPrivateKeyBytes(new Uint8Array(32).fill(7));
    const signed = await signRecordUpdate(
      update(wallet.address, [
        {
          kind: "write",
          member: indexerScalars.hash(new Uint8Array(32).fill(5) as Bytes32),
          mask: 0n,
          value: 0n,
          mintPolicy: {
            ...EMPTY_MINT_POLICY,
            cap: u64Max,
            limits: { ...EMPTY_MINT_POLICY.limits, volumeCap: u64Max },
          },
        },
      ]),
      wallet,
    );
    const bodies: string[] = [];
    const rpc = new RingRpc("http://ring.example", {
      allowInsecureHttp: true,
      fetch: (async (_url: unknown, init?: RequestInit) => {
        bodies.push(String(init?.body));
        const updateId = indexerScalars.hash(verifyRecordUpdate(signed));
        return new Response(JSON.stringify({ jsonrpc: "2.0", id: 1, result: { updateId } }), {
          headers: { "content-type": "application/json" },
        });
      }) as typeof globalThis.fetch,
    });
    await rpc.submitRecordUpdate(signed);
    expect(bodies[0]).toContain('"threshold":0,"cap":18446744073709551615,"cosignAbove":0,');
    expect(bodies[0]).toContain('"volumeCap":18446744073709551615,"volumeWindowSlots":0,');
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
  const otherSalt = new Uint8Array(32).fill(0x1d) as Bytes32;
  const zero = new Uint8Array(32) as Bytes32;
  const chain = () => {
    const ownerPkHash = memberOfTag(addressBytes(owner, "owner"));
    const listTag = listTagOf(listSalt);
    const ringListTag = listTagOf(ringSalt);
    return { ownerPkHash, listTag, ringListTag, enrolled: { ownerPkHash, listTag, ringListTag } };
  };
  const refusal = (reason: string) =>
    expect.objectContaining({ code: "RING_ENTRY_INVALID", details: { reason } });

  it("writes both keys on a ring that commits a ring list tag or refuses", () => {
    const { ownerPkHash, enrolled } = chain();
    expect(privateRevocationKeys({ listSalt, ringSalt }, enrolled)).toEqual([
      { member: privateListKey(ownerPkHash, listSalt), kind: "owner" },
      { member: ringListKey(ownerPkHash, ringSalt), kind: "ring" },
    ]);
    // A Ring RPC at an older revision, a faulty one or a hostile one yields no one-key clear.
    expect(() => privateRevocationKeys({ listSalt }, enrolled)).toThrowError(
      refusal("missingRingSalt"),
    );
    for (const wrong of [otherSalt, listSalt, zero])
      expect(() => privateRevocationKeys({ listSalt, ringSalt: wrong }, enrolled)).toThrowError(
        refusal("ringSaltUncommitted"),
      );
    expect(() => privateRevocationKeys({ ringSalt }, enrolled)).toThrowError(
      refusal("missingListSalt"),
    );
    for (const wrong of [otherSalt, ringSalt, zero])
      expect(() => privateRevocationKeys({ listSalt: wrong, ringSalt }, enrolled)).toThrowError(
        refusal("listSaltMismatch"),
      );
  });

  it("writes the owner key alone under a zero tag and the ring key alone with no row", () => {
    const { ownerPkHash, listTag, ringListTag } = chain();
    const zeroTag = { ownerPkHash, listTag, ringListTag: zero };
    for (const salts of [{ listSalt }, { listSalt, ringSalt }])
      expect(privateRevocationKeys(salts, zeroTag)).toEqual([
        { member: privateListKey(ownerPkHash, listSalt), kind: "owner" },
      ]);
    expect(privateRevocationKeys({ listSalt, ringSalt }, { ownerPkHash, ringListTag })).toEqual([
      { member: ringListKey(ownerPkHash, ringSalt), kind: "ring" },
    ]);
    // A service naming the ring salt as the owner's salt keys no entry a proof reads.
    expect(() =>
      privateRevocationKeys({ listSalt: ringSalt }, { ownerPkHash, ringListTag }),
    ).toThrowError(refusal("missingRingSalt"));
    expect(() =>
      privateRevocationKeys({ ringSalt }, { ownerPkHash, ringListTag: zero }),
    ).toThrowError(refusal("ringSaltUncommitted"));
  });

  it("refuses a key, a salt or a tag of another length", () => {
    const { ownerPkHash, enrolled } = chain();
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
      for (const tags of [
        { ...enrolled, ownerPkHash: bad },
        { ...enrolled, listTag: bad },
        { ...enrolled, ringListTag: bad },
      ])
        expect(() => privateRevocationKeys({ listSalt, ringSalt }, tags)).toThrowError(
          invalidLength,
        );
      expect(() => privateRevocationKeys({ listSalt: bad, ringSalt }, enrolled)).toThrowError(
        invalidLength,
      );
      expect(() => privateRevocationKeys({ listSalt, ringSalt: bad }, enrolled)).toThrowError(
        invalidLength,
      );
    }
  });
});
