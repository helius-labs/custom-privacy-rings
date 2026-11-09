import { address } from "@solana/kit";
import { describe, expect, it } from "vitest";
import type { Bytes32 } from "@heliuslabs/spr";
import { SprApi, indexerScalars } from "@heliuslabs/spr/internal";

import { QUAD_HEIGHT } from "../../src/protocol/membership/member-registry.js";
import { getEnrollmentQueueMethod } from "../../src/protocol/indexer/tracking-codec.js";
import { RingIndexer } from "../../src/rpc/indexer.js";

const { hash } = indexerScalars;
// base58 of 32 zero bytes.
const TAG = hash("11111111111111111111111111111111");
const REGISTRY = address("8hcir6LNDXjqKSof1KV41SBZxGaEQFB3WTtcoYn3Atpg");

describe("ring member registry wire", () => {
  const MEMBER = hash(new Uint8Array(32).fill(5) as Bytes32);
  const OTHER = hash(new Uint8Array(32).fill(6) as Bytes32);
  const CONTEXT = { slot: 1, blockTime: 2 };
  const level = () => [TAG, TAG, TAG];
  const quadPath = (index: number, levels = QUAD_HEIGHT) => ({
    index,
    siblings: Array.from({ length: levels }, level),
  });
  const record = {
    mintPolicy: {
      threshold: 0,
      cap: 0,
      cosignAbove: 0,
      withdrawalThreshold: 0,
      limits: {
        senderCap: 0,
        senderWindowSlots: 0,
        volumeCap: 0,
        volumeWindowSlots: 0,
        depositCap: 0,
        depositWindowSlots: 0,
        withdrawalCap: 0,
        withdrawalWindowSlots: 0,
      },
    },
    member: MEMBER,
    next: TAG,
    lists: 3,
  };
  const root = { root: TAG, revision: 1, nextIndex: 2 };
  const opening = { present: true, record, path: quadPath(1) };
  const openings = { context: CONTEXT, root, openings: [opening] };
  const indexerFor = (body: Readonly<Record<string, unknown>>) =>
    new RingIndexer(
      new SprApi({
        url: "https://indexer.example",
        fetch: async () => Response.json({ jsonrpc: "2.0", id: "test-account", ...body }),
      }),
    );
  const zero = new Uint8Array(32) as Bytes32;

  it("refuses an opening under another root or with a field a record no longer holds", async () => {
    const request = {
      registry: REGISTRY,
      members: [new Uint8Array(32).fill(5) as Bytes32],
      expectedRoot: zero,
    };
    const decoded = await indexerFor({ result: openings }).getStorageMemberOpenings(request);
    expect(decoded.openings[0]).toMatchObject({ kind: "present", path: { index: 1 } });
    expect(decoded.openings[0]?.path.siblings).toHaveLength(QUAD_HEIGHT);
    for (const changed of [
      { root: { ...root, root: OTHER } },
      { openings: [{ ...opening, record: { ...record, escrowKey: TAG } }] },
    ])
      await expect(
        indexerFor({ result: { ...openings, ...changed } }).getStorageMemberOpenings(request),
      ).rejects.toMatchObject({ code: "CLIENT_INVALID_RPC_RESPONSE" });
  });

  it("distinguishes every typed registry RPC failure", async () => {
    const request = { registry: REGISTRY, members: [zero], expectedRoot: zero };
    for (const [rpcCode, code] of [
      [-32070, "RING_REGISTRY_OUT_OF_SYNC"],
      [-32071, "RING_REGISTRY_ROOT_CHANGED"],
      [-32075, "RING_REGISTRY_PAGE_GAP"],
    ] as const) {
      const indexer = indexerFor({ error: { code: rpcCode, message: "private diagnostics" } });
      await expect(indexer.getStorageMemberOpenings(request)).rejects.toMatchObject({ code });
    }
    for (const unassigned of [-32072, -32073, -32076]) {
      const indexer = indexerFor({ error: { code: unassigned, message: "private diagnostics" } });
      await expect(indexer.getStorageMemberOpenings(request)).rejects.toMatchObject({
        code: "CLIENT_INDEXER",
      });
    }
  });

  it("decodes contiguous member pages", async () => {
    const request = { registry: REGISTRY, expectedRoot: zero, startIndex: 1, limit: 100 };
    const page = {
      context: CONTEXT,
      root: { ...root, nextIndex: 3 },
      records: [
        { record, path: quadPath(1) },
        { record: { ...record, member: OTHER }, path: quadPath(2) },
      ],
      nextLeafIndex: null,
    };
    const decoded = await indexerFor({ result: page }).getStorageMemberRecords(request);
    expect(decoded.records.map((entry) => entry.path.index)).toEqual([1, 2]);
    expect(decoded.nextLeafIndex).toBeUndefined();
  });
});

describe("tracking queue wire", () => {
  const base64 = (fill: number, length: number) =>
    Buffer.from(new Uint8Array(length).fill(fill)).toString("base64");
  const entry = (viewingU: string) => ({
    sequence: 7,
    owner: REGISTRY,
    member: TAG,
    proven: true,
    nullifierPk: TAG,
    listTag: TAG,
    viewingU,
    ephPk: base64(4, 33),
    ciphertext: base64(3, 64),
  });
  const page = (head: number, tail: number, viewingU = base64(0xff, 32)) => ({
    context: { slot: 1, blockTime: 2 },
    head,
    tail,
    entries: [entry(viewingU)],
  });

  it("reads a viewing key past the field from base64 and the queue bounds", () => {
    const decoded = getEnrollmentQueueMethod.decodeResponse(page(7, 9));
    expect(decoded).toMatchObject({ slot: 1n, head: 7n, tail: 9n });
    expect(decoded.entries[0]).toMatchObject({ sequence: 7n, proven: true });
    expect(decoded.entries[0]?.viewingU).toEqual(new Uint8Array(32).fill(0xff));
  });

  it("refuses a tail below the head and a viewing key off 32 bytes", () => {
    expect(() => getEnrollmentQueueMethod.decodeResponse(page(9, 7))).toThrow();
    expect(() => getEnrollmentQueueMethod.decodeResponse(page(7, 9, base64(1, 31)))).toThrow();
  });
});
