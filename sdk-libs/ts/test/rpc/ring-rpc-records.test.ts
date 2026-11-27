import { readFileSync } from "node:fs";

import { ed25519 } from "@noble/curves/ed25519.js";
import {
  createKeyPairSignerFromPrivateKeyBytes,
  getBase58Decoder,
  getBase58Encoder,
  getBase64Encoder,
  getTransactionDecoder,
  type Blockhash,
} from "@solana/kit";
import { describe, expect, it, vi } from "vitest";

import type { Address, Bytes32, Signature } from "@heliuslabs/spr/interface";
import { recordsCreationTransaction } from "../../src/operations/management/admin.js";
import { createRingRecords } from "../../src/operations/management/ring-records.js";
import { ringRecordsAddress } from "../../src/protocol/pda/index.js";
import {
  RingRpc,
  signCreateRecordsRegistryRequest,
  signRegisterEnrollmentRingRequest,
} from "../../src/rpc/ring-rpc.js";

const RING = "US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx" as Address;
const STORAGE_AUTHORITY = "8qbHbw2BbbTHBW1sbeqakYXVKRQM8Ne7pLK7m6CVfeR" as Address;
// `solana find-program-derived-address` over the storage program seeds of RING under STORAGE_AUTHORITY.
const RECORDS = "6e8wKXGefEfFUE4JbZXFeVZEasRKUU5Ag4a9mq919Er8" as Address;

const vector = JSON.parse(
  readFileSync(
    new URL("../../../../test-vectors/rpc/records_creation_message.json", import.meta.url),
    "utf8",
  ),
) as Readonly<{
  ring: Address;
  payer: Address;
  storageAuthority: Address;
  blockhash: Blockhash;
  message: string;
}>;

function answering(result: unknown): RingRpc {
  const fetch: typeof globalThis.fetch = () =>
    Promise.resolve(
      new Response(JSON.stringify({ jsonrpc: "2.0", id: 1, result }), {
        headers: { "content-type": "application/json" },
      }),
    );
  return new RingRpc("http://ring.example", { fetch, allowInsecureHttp: true });
}

describe("createRecordsRegistry", () => {
  it("builds the creation message byte for byte as the Rust vector", async () => {
    const transaction = await recordsCreationTransaction({
      ringProgramId: vector.ring,
      payer: vector.payer,
      storageAuthority: vector.storageAuthority,
      lifetime: { blockhash: vector.blockhash, lastValidBlockHeight: 1n },
    });
    expect(Buffer.from(transaction.messageBytes).toString("hex")).toBe(vector.message);
    expect(Object.keys(transaction.signatures)).toEqual([vector.payer, vector.storageAuthority]);
  });

  it("accepts only the ring's records and a creation the storage authority signed", async () => {
    const authority = await createKeyPairSignerFromPrivateKeyBytes(new Uint8Array(32).fill(8));
    const storageSecret = new Uint8Array(32).fill(5);
    const storageAuthority = getBase58Decoder().decode(
      ed25519.getPublicKey(storageSecret),
    ) as Address;
    const transaction = await recordsCreationTransaction({
      ringProgramId: RING,
      payer: authority.address,
      storageAuthority,
      lifetime: { blockhash: vector.blockhash, lastValidBlockHeight: 1n },
    });
    const message = new Uint8Array(transaction.messageBytes);
    const request = await signCreateRecordsRegistryRequest(
      {
        cluster: new Uint8Array(32).fill(1) as Bytes32,
        service: STORAGE_AUTHORITY,
        ringProgramId: RING,
      },
      authority,
      message,
    );
    const registry = await ringRecordsAddress(RING, storageAuthority);
    const signed = (secret: Uint8Array) =>
      getBase58Decoder().decode(ed25519.sign(message, secret)) as Signature;
    const signature = signed(storageSecret);
    await expect(
      answering({ registry, storageAuthority, signature }).createRecordsRegistry(request),
    ).resolves.toEqual({ registry, storageAuthority, signature });
    await expect(
      answering({ registry, storageAuthority }).createRecordsRegistry(request),
    ).resolves.toEqual({ registry, storageAuthority });
    await expect(
      answering({
        registry,
        storageAuthority,
        signature: signed(new Uint8Array(32).fill(6)),
      }).createRecordsRegistry(request),
    ).rejects.toMatchObject({
      code: "RING_RPC",
      details: { method: "createRecordsRegistry", reason: "invalid storage authority signature" },
    });
    // The service names the creator the message holds, another key's records are another registry.
    await expect(
      answering({ registry: RECORDS, storageAuthority: STORAGE_AUTHORITY }).createRecordsRegistry(
        request,
      ),
    ).rejects.toMatchObject({
      code: "RING_RPC",
      details: { method: "createRecordsRegistry", reason: "storageAuthority does not create" },
    });
    await expect(
      answering({ registry: STORAGE_AUTHORITY, storageAuthority }).createRecordsRegistry(request),
    ).rejects.toMatchObject({ code: "RING_RPC" });
  });
});

describe("createRingRecords", () => {
  it("sends a creation both keys signed, and asks nothing once the records exist or without a storage authority", async () => {
    const authority = await createKeyPairSignerFromPrivateKeyBytes(new Uint8Array(32).fill(8));
    const storageSecret = new Uint8Array(32).fill(5);
    const storageAuthority = getBase58Decoder().decode(
      ed25519.getPublicKey(storageSecret),
    ) as Address;
    const registry = await ringRecordsAddress(RING, storageAuthority);
    const sent: Uint8Array[] = [];
    const asked: unknown[] = [];
    const flow = (
      named: Address | undefined,
      existing: boolean,
    ): Parameters<typeof createRingRecords>[0] => ({
      client: {
        solanaRpc: {
          sendTransaction: (wire: string) => ({
            send: async () => {
              sent.push(new Uint8Array(getBase64Encoder().encode(wire)));
              return "1".repeat(64);
            },
          }),
        } as never,
        commitment: "confirmed",
        getLatestBlockhash: async () => ({ blockhash: vector.blockhash, lastValidBlockHeight: 1n }),
        getAccount: async (account: Address) =>
          existing && account === registry ? ({} as never) : undefined,
        confirmTransaction: async () => 1n,
      },
      ringRpc: {
        health: async () =>
          (named === undefined ? {} : { storageAuthorityPublicKey: named }) as never,
        createRecordsRegistry: async (request) => {
          asked.push(request);
          const message = new Uint8Array(getBase64Encoder().encode(request.message));
          return {
            registry,
            storageAuthority,
            signature: getBase58Decoder().decode(ed25519.sign(message, storageSecret)) as Signature,
          };
        },
      },
      target: {
        cluster: new Uint8Array(32).fill(1) as Bytes32,
        service: STORAGE_AUTHORITY,
        ringProgramId: RING,
      },
      authority,
    });

    await expect(createRingRecords(flow(storageAuthority, false))).resolves.toBe(registry);
    const [wire] = sent;
    if (wire === undefined) throw new Error("nothing sent");
    const { messageBytes, signatures } = getTransactionDecoder().decode(wire);
    for (const [signer, signature] of Object.entries(signatures)) {
      if (signature === null) throw new Error(`${signer} did not sign`);
      expect(
        ed25519.verify(
          signature,
          new Uint8Array(messageBytes),
          new Uint8Array(getBase58Encoder().encode(signer)),
        ),
      ).toBe(true);
    }
    expect(Object.keys(signatures)).toEqual([authority.address, storageAuthority]);

    await expect(createRingRecords(flow(storageAuthority, true))).resolves.toBe(registry);
    await expect(createRingRecords(flow(undefined, false))).rejects.toMatchObject({
      code: "RING_RPC",
      details: { method: "health", reason: "no storage authority" },
    });
    expect([asked.length, sent.length]).toEqual([1, 1]);
  });
});

describe("registerEnrollmentRing", () => {
  it("posts the policy authority's request and accepts only an empty answer", async () => {
    const authority = await createKeyPairSignerFromPrivateKeyBytes(new Uint8Array(32).fill(8));
    const request = await signRegisterEnrollmentRingRequest(
      {
        cluster: new Uint8Array(32).fill(1) as Bytes32,
        service: STORAGE_AUTHORITY,
        ringProgramId: RING,
      },
      authority,
      RECORDS,
    );
    const answer = (result: unknown) =>
      vi.fn(async (_url: string, init: RequestInit) => {
        const body = JSON.parse(String(init.body)) as {
          id: unknown;
          method: string;
          params: { ring: Address; auth: { authority: Address } };
        };
        expect(body.method).toBe("registerEnrollmentRing");
        expect(Object.keys(body.params)).toEqual(["ring", "auth"]);
        expect(body.params.auth.authority).toBe(authority.address);
        return new Response(JSON.stringify({ jsonrpc: "2.0", id: body.id, result }), {
          headers: { "content-type": "application/json" },
        });
      });
    const ok = new RingRpc("https://ring-rpc.test", { fetch: answer({}) as never });
    await expect(ok.registerEnrollmentRing(request)).resolves.toBeUndefined();
    const extra = new RingRpc("https://ring-rpc.test", { fetch: answer({ rings: 1 }) as never });
    await expect(extra.registerEnrollmentRing(request)).rejects.toThrow();
  });
});
