import { getBase58Decoder, type Signature } from "@solana/kit";
import { describe, expect, it } from "vitest";

import { initializePoseidon } from "@heliuslabs/spr";
import {
  encodeMergeOutputData,
  encryptMergeOutput,
  mergeOutputBlinding,
  mergeOutputKey,
  poseidon,
  ZERO_32,
  ZERO_NULLIFIER_PK,
} from "@heliuslabs/spr/internal";
import type { Bytes16, Bytes32 } from "@heliuslabs/spr/interface";
import { randomSalt, ShieldedAddress, ViewingKey, X25519Keypair } from "@heliuslabs/spr/keypair";
import { SOL_MINT, Utxo, Wallet, decryptTransactions } from "@heliuslabs/spr/transaction";
import { ringNoteCodec } from "../../src/state/ring-sync.js";

import {
  AnonymousSync,
  type AnonymousScan,
  type AnonymousStream,
} from "../../src/state/anonymous-sync.js";
import { AnonymousWallet } from "../../src/state/anonymous-wallet.js";
import { TAG_WINDOW, ViewingHistory } from "../../src/state/viewing-history.js";
import {
  bootstrapSlotTag,
  deliveredSlotSecret,
  slotTag,
} from "../../src/crypto/derivation/ring-stream.js";
import { encodeRingBodyOutput } from "../../src/protocol/transaction/output.js";
import { TREE, TREE_ID } from "../helpers/ring-pipeline.js";
import {
  ASSETS,
  field,
  found,
  hex,
  keypair,
  output,
  RING,
  ScanIndexer,
  ScanWallet,
  sealedNote,
} from "../helpers/ring-scan.js";
import { RING_RECORD_LEN } from "../../src/protocol/codec/ring-disclosure-layout.js";

await initializePoseidon();

const ZERO = new Uint8Array(32) as Bytes32;

function stream(scan: AnonymousScan, utxoHash: Bytes32): AnonymousStream {
  const entry = scan.found.find((utxo) => hex(utxo.utxoHash) === hex(utxoHash));
  if (entry === undefined) throw new Error(`note ${hex(utxoHash)} not found`);
  return entry.stream;
}

describe("anonymous sync", () => {
  it("finds a payment and the sender's change with no use of a tag", async () => {
    const indexer = new ScanIndexer();
    const alice = new ScanWallet(1);
    const bob = new ScanWallet(2);
    const paid = await alice.pay(indexer, [{ to: bob.address, amount: 30n }]);
    const received = await bob.sync(indexer, bob.fresh());
    expect(stream(received, output(paid, 1))).toEqual({ kind: "fullScan" });
    expect(received.utxos.map((note) => note.utxo.amount)).toEqual([30n]);
    const change = await alice.sync(indexer, alice.fresh());
    expect(stream(change, output(paid, 0))).toEqual({ kind: "fullScan" });
    expect(change.utxos.map((note) => note.utxo.amount)).toEqual([70n]);
    expect(indexer.transactionTagReads).toBe(0);
  });

  it("opens a payment, the sender's change and nothing for a stranger through core", async () => {
    const indexer = new ScanIndexer();
    const alice = new ScanWallet(1);
    const bob = new ScanWallet(2);
    const paid = await alice.pay(indexer, [{ to: bob.address, amount: 30n }]);
    const credited = async (wallet: ScanWallet) => {
      const core = new Wallet({ identity: wallet.address });
      await decryptTransactions({
        wallet: core,
        keys: wallet.keys,
        transactions: [paid],
        ring: ringNoteCodec({ ringProgramId: RING, keys: wallet.keys, assets: ASSETS }),
      });
      return core.ringBalances().flatMap((ring) => ring.assets.map((asset) => asset.amount));
    };
    expect(await credited(bob)).toEqual([30n]);
    expect(await credited(alice)).toEqual([70n]);
    expect(await credited(new ScanWallet(3))).toEqual([]);
  });

  it("skips a body sealed to another key and goes on", async () => {
    const indexer = new ScanIndexer();
    const alice = new ScanWallet(1);
    const bob = new ScanWallet(2);
    const stranger = ShieldedAddress.fromPublicKeys(
      bob.address.signingPublicKey,
      bob.address.nullifierPublicKey,
      keypair(9).shieldedAddress().viewingPublicKey,
    );
    await alice.pay(indexer, [{ to: stranger, amount: 9n }]);
    const paid = await alice.pay(indexer, [{ to: bob.address, amount: 30n }]);
    const scanned = await bob.sync(indexer, bob.fresh());
    expect(found(scanned)).toEqual([hex(output(paid, 1))]);
    expect(scanned.unmatched).toEqual([]);
  });

  it("finds change on no-change and two-output frames", async () => {
    const indexer = new ScanIndexer();
    const carol = new ScanWallet(3);
    const bob = new ScanWallet(4);
    const noChange = await carol.pay(indexer, [{ to: bob.address, amount: 100n }]);
    const withChange = await carol.pay(indexer, [{ to: bob.address, amount: 40n }]);
    const scan = await carol.sync(indexer, carol.fresh());
    expect(scan.utxos.map((note) => note.utxo.amount)).toEqual([0n, 60n]);
    expect(found(scan)).toEqual([hex(output(noChange, 0)), hex(output(withChange, 0))]);
    expect(scan.sent.map((row) => row.amount)).toEqual([100n, 40n]);
  });

  it("lists a sent payment and its change once when the indexer repeats the row", async () => {
    const indexer = new ScanIndexer();
    const carol = new ScanWallet(3);
    const bob = new ScanWallet(4);
    const paid = await carol.pay(indexer, [{ to: bob.address, amount: 40n }]);
    indexer.publish(paid);
    const scan = await carol.sync(indexer, carol.fresh());
    expect(scan.utxos.map((note) => note.utxo.amount)).toEqual([60n]);
    expect(scan.sent.map((row) => [hex(row.utxoHash), row.amount])).toEqual([
      [hex(output(paid, 1)), 40n],
    ]);
  });

  it("lists every payment in the change and verifies a row only under a held address", async () => {
    const indexer = new ScanIndexer();
    const alice = new ScanWallet(1);
    const bob = new ScanWallet(2).address;
    const carol = new ScanWallet(3).address;
    const paid = await alice.pay(indexer, [
      { to: bob, amount: 30n },
      { to: carol, amount: 20n },
    ]);
    const forged = ShieldedAddress.fromPublicKeys(
      carol.signingPublicKey,
      new ScanWallet(4).address.nullifierPublicKey,
      carol.viewingPublicKey,
    );
    const scan = await AnonymousSync.create(alice.keys, ASSETS)
      .withRecipientAddresses([bob, forged])
      .fetch(indexer.client(), alice.fresh());
    expect(
      scan.sent.map((row) => [
        row.position,
        hex(row.recipientViewingPublicKey.toBytes()),
        row.ownerPublicKey === undefined ? undefined : hex(row.ownerPublicKey.toBytes()),
        row.amount,
        row.check,
        hex(row.utxoHash),
        row.txSignature,
      ]),
    ).toEqual([
      [
        1,
        hex(bob.viewingPublicKey.toBytes()),
        hex(bob.signingPublicKey.toBytes()),
        30n,
        { kind: "verified", mint: SOL_MINT },
        hex(output(paid, 1)),
        paid.txSignature,
      ],
      [
        2,
        hex(carol.viewingPublicKey.toBytes()),
        undefined,
        20n,
        { kind: "unverified" },
        hex(output(paid, 2)),
        paid.txSignature,
      ],
    ]);
    const unheld = await alice.sync(indexer, alice.fresh());
    expect(unheld.sent.map((row) => row.check)).toEqual([
      { kind: "unverified" },
      { kind: "unverified" },
    ]);
  });

  it("finds change under another transaction key, listing no sent row", async () => {
    const indexer = new ScanIndexer();
    const alice = new ScanWallet(1);
    const bob = new ScanWallet(2);
    const relayed = alice.seal(indexer, [{ to: bob.address, amount: 30n }]);
    const keys = {
      ...alice.keys,
      address: () => alice.keys.address(),
      viewingPublicKeys: () => alice.keys.viewingPublicKeys(),
      decrypt: alice.keys.decrypt.bind(alice.keys),
      derive: alice.keys.derive.bind(alice.keys),
      ringDeliverySecrets: alice.keys.ringDeliverySecrets.bind(alice.keys),
      viewTags: alice.keys.viewTags.bind(alice.keys),
      // A relayer drew the frame's key, the wallet's own derivation names another.
      transactionKeys: async (requests: readonly unknown[]) =>
        requests.map(() => ViewingKey.generate()),
    };
    indexer.publish(relayed);
    const scan = await AnonymousSync.create(keys, ASSETS).fetch(indexer.client(), alice.fresh());
    expect(stream(scan, output(relayed, 0))).toEqual({ kind: "fullScan" });
    expect(scan.sent).toEqual([]);
  });

  it("leaves no row for a listed body that fails to open, the change still found", async () => {
    const indexer = new ScanIndexer();
    const alice = new ScanWallet(1);
    const bob = new ScanWallet(2).address;
    const paid = alice.seal(indexer, [{ to: bob, amount: 30n }]);
    const slot = paid.outputSlots[1];
    if (slot === undefined) throw new Error("no payment");
    const payload = slot.payload.slice(0, -1);
    new DataView(payload.buffer).setUint32(1, payload.length - 5, true);
    indexer.publish({ ...paid, outputSlots: paid.outputSlots.with(1, { ...slot, payload }) });
    const scan = await alice.sync(indexer, alice.fresh());
    expect(scan.sent).toEqual([]);
    expect(stream(scan, output(paid, 0))).toEqual({ kind: "fullScan" });
  });

  it("never fails on sender-chosen garbage sealed to the wallet's key", async () => {
    const indexer = new ScanIndexer();
    const alice = new ScanWallet(1);
    const bob = new ScanWallet(2);
    const txKey = ViewingKey.generate();
    const salt = randomSalt();
    const firstNullifier = field(9, 9);
    const secret = (outputIndex: number) =>
      deliveredSlotSecret({
        agreement: txKey.ecdh(bob.viewingKey),
        txViewingKey: txKey.publicKey(),
        recipientViewingKey: bob.viewingKey,
        salt,
        outputIndex,
        firstNullifier,
      });
    const noise = new Uint8Array(RING_RECORD_LEN).fill(0x5c);
    // Opens under bob's key, its blinding outside the field fails the hash rebuild.
    const named = sealedNote(secret(2), {
      owner: bob.address,
      blinding: new Uint8Array(32).fill(0xff) as Bytes32,
      amount: 5n,
      mint: SOL_MINT,
    });
    const payloads = [
      new Uint8Array(),
      encodeRingBodyOutput(noise, new Uint8Array()),
      encodeRingBodyOutput(named, new Uint8Array()),
    ];
    const leaf = indexer.leaves(payloads.length);
    const unmatched = new Uint8Array(32).fill(0x0c) as Bytes32;
    indexer.publish({
      slot: 1n,
      txSignature: getBase58Decoder().decode(new Uint8Array(64).fill(0x77)) as Signature,
      eventIndex: 0,
      txViewingPublicKey: txKey.publicKey(),
      salt,
      outputSlots: payloads.map((payload, position) => ({
        viewTag: bootstrapSlotTag(bob.viewingKey.u()),
        outputContext: {
          hash: unmatched,
          tree: TREE,
          treeId: TREE_ID,
          leafIndex: leaf + BigInt(position),
        },
        payload,
      })),
      messages: [],
      nullifiers: [firstNullifier],
      proofless: false,
      ringProgramId: RING,
    });
    const paid = await alice.pay(indexer, [{ to: bob.address, amount: 30n }]);
    const scanned = await bob.sync(indexer, bob.fresh());
    expect(found(scanned)).toEqual([hex(output(paid, 1))]);
    expect(scanned.unmatched).toEqual([]);
  });

  it("finds a merge of the wallet's notes and marks them spent", async () => {
    const indexer = new ScanIndexer();
    const alice = new ScanWallet(1);
    const bob = new ScanWallet(2);
    for (const amount of [70n, 30n]) await alice.pay(indexer, [{ to: bob.address, amount }]);
    const held = await bob.sync(indexer, bob.fresh());
    const nullifiers = held.utxos.map((utxo) => utxo.nullifier);
    const [first] = nullifiers;
    if (first === undefined) throw new Error("no notes");
    const nullifierKey = bob.keypair.nullifierKey();
    const merged = new Utxo({
      owner: bob.address.signingPublicKey,
      asset: SOL_MINT,
      amount: 100n,
      blinding: mergeOutputBlinding(nullifierKey, first),
      ringProgramId: RING,
    });
    const mergedHash = merged.hash(bob.address.nullifierPublicKey, TREE_ID);
    const salt = new Uint8Array(16).fill(8) as Bytes16;
    indexer.publish({
      slot: 99n,
      txSignature: getBase58Decoder().decode(new Uint8Array(64).fill(0x99)) as Signature,
      eventIndex: 0,
      outputSlots: [
        {
          viewTag: first,
          outputContext: {
            hash: mergedHash,
            tree: TREE,
            treeId: TREE_ID,
            leafIndex: indexer.leaves(1),
          },
          payload: encodeMergeOutputData({
            salt,
            ciphertext: encryptMergeOutput(
              { amount: 100n, mint: SOL_MINT },
              mergeOutputKey(nullifierKey, first),
              salt,
            ),
            ringDataHash: ZERO,
          }),
        },
      ],
      messages: [],
      nullifiers,
      proofless: false,
      ringProgramId: RING,
    });
    const next = await bob.sync(indexer, held.history, held.utxos);
    expect(stream(next, mergedHash)).toEqual({ kind: "merge", firstNullifier: first });
    expect(next.spentNullifiers.map(hex)).toEqual(expect.arrayContaining(nullifiers.map(hex)));
    expect(
      next.utxos.filter((utxo) => !utxo.spent).map((utxo) => hex(utxo.outputContext.hash)),
    ).toEqual([hex(mergedHash)]);
    const wallet = AnonymousWallet.create(
      new Wallet({ identity: bob.address, registry: ASSETS }),
      bob.fresh(),
    );
    await wallet.absorb(held);
    await wallet.absorb(next);
    await expect(wallet.absorb(next)).rejects.toThrow("RING_SCAN_CONSUMED");
    await expect(wallet.absorb({ ...next })).rejects.toThrow("RING_SCAN_CONSUMED");
    const repeated = await bob.sync(indexer, wallet.history(), wallet.notes());
    await wallet.absorb(repeated);
    expect(wallet.notes().map((utxo) => [hex(utxo.outputContext.hash), utxo.spent])).toEqual([
      ...held.utxos.map((utxo) => [hex(utxo.outputContext.hash), true]),
      [hex(mergedHash), false],
    ]);
  });

  it("reports only the wallet's own spends out of a whole ring's rows", async () => {
    const indexer = new ScanIndexer();
    const alice = new ScanWallet(1);
    const bob = new ScanWallet(2);
    const carol = new ScanWallet(3);
    const strangers = await alice.pay(indexer, [{ to: carol.address, amount: 5n }]);
    await alice.pay(indexer, [{ to: bob.address, amount: 5n }]);
    const scan = await bob.sync(indexer, bob.fresh());
    expect(scan.spentNullifiers).toEqual([]);
    expect(strangers.nullifiers.length).toBeGreaterThan(0);
  });

  it("derives a zero nullifier key note's nullifier with no holder", async () => {
    const indexer = new ScanIndexer();
    const alice = new ScanWallet(1);
    const bob = new ScanWallet(2);
    const zeroKeyed = ShieldedAddress.fromPublicKeys(
      bob.address.signingPublicKey,
      ZERO_NULLIFIER_PK as Bytes32,
      bob.viewingKey,
    );
    const paid = await alice.pay(indexer, [{ to: zeroKeyed, amount: 30n }]);
    const scan = await bob.sync(indexer, bob.fresh());
    const [note] = scan.utxos;
    if (note === undefined) throw new Error("no note");
    const hash = output(paid, 1);
    expect(hex(note.outputContext.hash)).toBe(hex(hash));
    expect(scan.found[0]?.nullifierPublicKey).toEqual(ZERO_NULLIFIER_PK);
    expect(note.nullifier).toEqual(poseidon([hash, note.utxo.blinding, ZERO_32]));
  });

  it("rebuilds an entry's moved note and its change from their records under the sender's key", async () => {
    const indexer = new ScanIndexer();
    const alice = new ScanWallet(1);
    const entered = await alice.pay(indexer, [{ to: alice.address, amount: 25n }]);
    const scan = await alice.sync(indexer, alice.fresh());
    expect(found(scan)).toEqual([hex(output(entered, 0)), hex(output(entered, 1))]);
    expect(scan.utxos.map((note) => note.utxo.amount).sort()).toEqual([25n, 75n]);
    expect(scan.utxos.every((note) => note.utxo.ringProgramId !== undefined)).toBe(true);
  });

  it("rebuilds a zero amount change slot", async () => {
    const indexer = new ScanIndexer();
    const alice = new ScanWallet(1);
    const bob = new ScanWallet(2).address;
    const paid = await alice.pay(indexer, [{ to: bob, amount: 100n }]);
    expect(paid.outputSlots[0]?.payload.length).toBeGreaterThan(0);
    const scan = await alice.sync(indexer, alice.fresh());
    expect(found(scan)).toEqual([hex(output(paid, 0))]);
    expect(scan.utxos[0]?.utxo.amount).toBe(0n);
  });

  it("holds a reply as a claim, a confirmation clears it and moves no counter", async () => {
    const indexer = new ScanIndexer();
    const alice = new ScanWallet(1);
    const bob = new ScanWallet(2);
    const carol = new ScanWallet(3);
    await carol.pay(indexer, [{ to: bob.address, amount: 5n }]);
    const bare = await bob.sync(indexer, bob.fresh());
    expect(bare.senderClaims).toEqual([]);

    const paid = await alice.pay(indexer, [
      { to: bob.address, amount: 30n, reply: alice.replyTo() },
    ]);
    const scan = await bob.sync(indexer, bob.fresh());
    const claim = scan.senderClaims[0];
    if (claim === undefined) throw new Error("no claim");
    expect(hex(claim.utxoHash)).toBe(hex(output(paid, 1)));

    const wallet = AnonymousWallet.create(
      new Wallet({ identity: bob.address, registry: ASSETS }),
      bob.fresh(),
    );
    await wallet.absorb(scan);
    expect(Object.isFrozen(scan)).toBe(true);
    expect(wallet.pendingClaims().map((pending) => hex2(pending.claimedViewingKey))).toEqual([
      hex2(alice.viewingKey),
    ]);
    const stored = wallet.history().toJson();
    expect(() => wallet.confirmClaim(carol.viewingKey)).toThrow("RING_UNKNOWN_SENDER_CLAIM");
    wallet.confirmClaim(alice.viewingKey);
    // A reply is an unauthenticated claim, it pairs no sender and takes no tag.
    expect(wallet.history().toJson()).toBe(stored);
    expect(wallet.pendingClaims()).toEqual([]);
    expect(() => wallet.confirmClaim(alice.viewingKey)).toThrow("RING_UNKNOWN_SENDER_CLAIM");
  });

  it("reads a decoded reply as a claim and an undecodable one as none, the note kept", async () => {
    const indexer = new ScanIndexer();
    const alice = new ScanWallet(1);
    const bob = new ScanWallet(2);
    const carol = new ScanWallet(3);
    const paid = await alice.pay(indexer, [
      { to: bob.address, amount: 30n, reply: alice.replyTo() },
      { to: carol.address, amount: 20n },
    ]);
    const decodes = await bob.sync(indexer, bob.fresh());
    expect(found(decodes)).toEqual([hex(output(paid, 1))]);
    expect(decodes.senderClaims.map(({ claim }) => claim.claimed.viewingKey.u())).toEqual([
      alice.viewingKey.u(),
    ]);
    expect(decodes.senderClaims[0]?.claim.claimed.ownerKey).toEqual(
      alice.address.signingPublicKey.confidentialViewTag(),
    );
    const undecodable = await carol.sync(indexer, carol.fresh());
    expect(found(undecodable)).toEqual([hex(output(paid, 2))]);
    expect(undecodable.senderClaims).toEqual([]);
  });
});

function hex2(key: Readonly<{ toBytes(): Uint8Array }>): string {
  return hex(key.toBytes());
}

describe("viewing history", () => {
  const key = () => X25519Keypair.generate().publicKey();
  const holder = (seed: number) => {
    const wallet = new ScanWallet(seed);
    const fresh = ViewingHistory.create(wallet.viewingKey, 0n);
    return { wallet, fresh, viewing: wallet.keypair.viewingKey() };
  };

  it("writes the stored form key for key, each ring by address bytes as Rust orders them", () => {
    // Bytes 0x06.. sort ahead of 0xff.., the base58 text of the second sorts first.
    const ringA = "So11111111111111111111111111111111111111112" as typeof RING;
    const ringB = getBase58Decoder().decode(new Uint8Array(32).fill(0xff)) as typeof RING;
    expect(ringB < ringA).toBe(true);
    const viewingPublicKey = key();
    const history = ViewingHistory.create(viewingPublicKey, 3n)
      .withRing(ringB, 8n)
      .withRing(ringA)
      .withRequestCount(5n);
    expect(history.toJson()).toBe(
      `{"viewingHistory":[{"viewingPubkey":"${hex2(viewingPublicKey)}","createdAt":3,` +
        `"tx":{"next":0,"scanned":0},"request":{"next":5,"scanned":0},` +
        `"ringPositions":[{"ring":"${ringA}","afterSlot":null},{"ring":"${ringB}","afterSlot":8}],"tagPosition":null}]}`,
    );
    expect(ViewingHistory.fromJson(history.toJson()).toJson()).toBe(history.toJson());
  });

  it("refuses another field, a stale rotation, a malformed counter and a repeated ring", () => {
    const history = ViewingHistory.create(key(), 0n);
    const stored = JSON.parse(history.toJson()) as object;
    expect(() => ViewingHistory.fromJson(JSON.stringify({ ...stored, other: 7 }))).toThrow(
      "RING_VIEWING_HISTORY_INVALID",
    );
    const rotated = history.withRotatedKey(key(), 5n);
    expect(() => rotated.withRotatedKey(key(), 5n)).toThrow("RING_ROTATED_KEY_NOT_NEWER");
    const counter = '"request":{"next":0,"scanned":0}';
    expect(history.toJson()).toContain(counter);
    for (const refused of [
      '"request":{"next":1,"scanned":2}',
      '"request":{"next":0,"scanned":0,"pending":[]}',
      '"request":{"next":0}',
      '"request":{"next":-1,"scanned":0}',
      `"request":{"next":${1n << 64n},"scanned":0}`,
      '"knownSenders":[]',
    ])
      expect(
        () => ViewingHistory.fromJson(history.toJson().replace(counter, refused)),
        refused,
      ).toThrow("RING_VIEWING_HISTORY_INVALID");
    const enrolled = history.withRing(RING);
    const row = `{"ring":"${RING}","afterSlot":null}`;
    expect(() => ViewingHistory.fromJson(enrolled.toJson().replace(row, `${row},${row}`))).toThrow(
      "RING_VIEWING_HISTORY_INVALID",
    );
    expect(() =>
      ViewingHistory.fromJson(enrolled.toJson().replace(`"ring":"${RING}"`, `"ring":"nope"`)),
    ).toThrow("RING_VIEWING_HISTORY_INVALID");
  });

  it("reads a u64 counter past 2^53 exactly", () => {
    const history = ViewingHistory.create(key(), 0n);
    const json = history.toJson().replace('"tx":{"next":0', `"tx":{"next":${(1n << 64n) - 2n}`);
    expect(ViewingHistory.fromJson(json).current().txCount).toBe((1n << 64n) - 2n);
  });

  it("takes the next sender index on each selection", async () => {
    const { wallet, fresh, viewing } = holder(1);
    const first = await fresh.select(wallet.keys);
    const second = await first.history.select(wallet.keys);
    expect(first.tag.viewingPublicKey.equals(wallet.viewingKey)).toBe(true);
    expect([first, second].map(({ tag }) => hex(tag.sender))).toEqual(
      [0n, 1n].map((index) => hex(slotTag(viewing.senderViewTag(index)))),
    );
    expect(hex(first.tag.sender)).not.toBe(hex(second.tag.sender));
    expect([first, second].map(({ history }) => history.current().txCount)).toEqual([1n, 2n]);
    expect(second.history.current().requestCount).toBe(0n);
    const ceiling = ViewingHistory.fromJson(
      fresh.toJson().replace('"tx":{"next":0', `"tx":{"next":${(1n << 64n) - 1n}`),
    );
    await expect(ceiling.select(wallet.keys)).rejects.toThrow("RING_COUNTER_OVERFLOW");
    viewing.destroy();
  });

  it("mints each request tag at the counter and refuses nothing but a counter at the u64 ceiling", async () => {
    const { wallet, fresh, viewing } = holder(1);
    let history = fresh;
    // No index past the window is refused, a sync lists the window past the stored counter.
    for (let index = 0n; index <= TAG_WINDOW; index += 1n) {
      const minted = await history.requestTag(wallet.keys);
      expect(minted.tag, String(index)).toEqual(slotTag(viewing.recipientRequestViewTag(index)));
      history = minted.history;
    }
    expect(history.current().requestCount).toBe(TAG_WINDOW + 1n);
    expect(history.current().txCount).toBe(0n);
    const ceiling = ViewingHistory.fromJson(
      fresh.toJson().replace('"request":{"next":0', `"request":{"next":${(1n << 64n) - 1n}`),
    );
    await expect(ceiling.requestTag(wallet.keys)).rejects.toThrow("RING_COUNTER_OVERFLOW");
    viewing.destroy();
  });

  it("raises the request counter to a count the wallet states and never lowers it", async () => {
    const { wallet, fresh, viewing } = holder(1);
    const restored = fresh.withRequestCount(70n);
    expect(restored.current().requestCount).toBe(70n);
    expect(restored.withRequestCount(3n).current().requestCount).toBe(70n);
    expect((await restored.requestTag(wallet.keys)).tag).toEqual(
      slotTag(viewing.recipientRequestViewTag(70n)),
    );
    for (const count of [-1n, 1n << 64n])
      expect(() => fresh.withRequestCount(count)).toThrow("RING_VIEWING_HISTORY_INVALID");
    viewing.destroy();
  });
});
