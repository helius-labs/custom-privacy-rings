import { createKeyPairSignerFromPrivateKeyBytes } from "@solana/kit";
import { describe, expect, it, vi } from "vitest";

import type { Address, Bytes32 } from "@heliuslabs/spr/interface";
import {
  RingRpc,
  signCreateRecordsRegistryRequest,
  signRegisterEnrollmentRingRequest,
} from "../../src/rpc/ring-rpc.js";

const RING = "US517G5965aydkZ46HS38QLi7UQiSojurfbQfKCELFx" as Address;
const STORAGE_AUTHORITY = "8qbHbw2BbbTHBW1sbeqakYXVKRQM8Ne7pLK7m6CVfeR" as Address;
// `solana find-program-derived-address` over the storage program seeds of RING under STORAGE_AUTHORITY.
const RECORDS = "6e8wKXGefEfFUE4JbZXFeVZEasRKUU5Ag4a9mq919Er8" as Address;

function answering(registry: Address): RingRpc {
  const fetch: typeof globalThis.fetch = () =>
    Promise.resolve(
      new Response(
        JSON.stringify({
          jsonrpc: "2.0",
          id: 1,
          result: { registry, storageAuthority: STORAGE_AUTHORITY },
        }),
        { headers: { "content-type": "application/json" } },
      ),
    );
  return new RingRpc("http://ring.example", { fetch, allowInsecureHttp: true });
}

describe("createRecordsRegistry", () => {
  it("accepts only the ring's records under the storage authority it names", async () => {
    const authority = await createKeyPairSignerFromPrivateKeyBytes(new Uint8Array(32).fill(8));
    const request = await signCreateRecordsRegistryRequest(
      {
        cluster: new Uint8Array(32).fill(1) as Bytes32,
        service: STORAGE_AUTHORITY,
        ringProgramId: RING,
      },
      authority,
    );
    await expect(answering(RECORDS).createRecordsRegistry(request)).resolves.toEqual({
      registry: RECORDS,
      storageAuthority: STORAGE_AUTHORITY,
    });
    await expect(answering(STORAGE_AUTHORITY).createRecordsRegistry(request)).rejects.toMatchObject(
      { code: "RING_RPC" },
    );
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
