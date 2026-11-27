import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { RingRpcRefusal } from "../../../src/protocol/ring-rpc/ring-rpc-refusal.js";
import { RingRpcMethod } from "../../../src/protocol/ring-rpc/ring-rpc-types.js";

const vector = JSON.parse(
  readFileSync(
    new URL("../../../../../test-vectors/rpc/ring_rpc_methods_and_codes.json", import.meta.url),
    "utf8",
  ),
) as { methods: string[]; refusals: { code: number; name: string }[] };

const ascending = (codes: readonly number[]) => [...codes].sort((a, b) => a - b);

describe("the Ring RPC method and refusal vector", () => {
  it("registers every method RingRpcMethod names", () => {
    expect(vector.methods).toEqual(expect.arrayContaining(Object.values(RingRpcMethod)));
  });

  it("holds exactly the refusal codes RingRpcRefusal names", () => {
    expect(ascending(Object.values(RingRpcRefusal))).toEqual(
      ascending(vector.refusals.map(({ code }) => code)),
    );
  });
});
