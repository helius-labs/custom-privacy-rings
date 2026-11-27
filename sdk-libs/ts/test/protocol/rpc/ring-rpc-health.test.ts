import { describe, expect, it } from "vitest";

import { decodeRingRpcHealth } from "../../../src/rpc/ring-rpc/health.js";

const KEY = "11111111111111111111111111111111";
const WRITER = "4Nd1mBQtrMJVYVfKf2PJy9NZUZdTAsp7D4xWLs4gDB4T";
const SWITCHES = {
  relay: true,
  recordWriter: false,
  enrollmentWrites: true,
  readerGrants: "database",
};

describe("Ring RPC health", () => {
  it("reads the role keys the service runs, its switches and its reader grant mode", () => {
    expect(
      decodeRingRpcHealth({
        mode: "derived",
        servicePubkey: KEY,
        feePayerPubkey: KEY,
        enrollmentWriterPubkey: WRITER,
        ...SWITCHES,
      }),
    ).toEqual({
      mode: "derived",
      servicePublicKey: KEY,
      feePayerPublicKey: KEY,
      enrollmentWriterPublicKey: WRITER,
      relay: true,
      recordWriter: false,
      enrollmentWrites: true,
      readerGrants: "database",
    });
    expect(
      decodeRingRpcHealth({
        mode: "local",
        servicePubkey: KEY,
        enrollmentWriterPubkey: null,
        ...SWITCHES,
      }),
    ).not.toHaveProperty("enrollmentWriterPublicKey");
  });

  const WIRE = { mode: "local", servicePubkey: KEY, enrollmentWriterPubkey: null, ...SWITCHES };

  it.each([
    ["an unknown field", { newerField: 1 }, "result.shape"],
    ["a null skipped role key", { coSignerPubkey: null }, "result.coSignerPubkey"],
  ])("refuses %s", (_name, change, path) => {
    expect(() => decodeRingRpcHealth({ ...WIRE, ...change })).toThrow(
      expect.objectContaining({ code: "RING_RPC", details: expect.objectContaining({ path }) }),
    );
  });

  it.each(["relay", "recordWriter", "enrollmentWrites", "readerGrants", "enrollmentWriterPubkey"])(
    "refuses a result without %s",
    (field) => {
      const wire: Record<string, unknown> = { ...WIRE };
      delete wire[field];
      expect(() => decodeRingRpcHealth(wire)).toThrow(
        expect.objectContaining({
          code: "RING_RPC",
          details: expect.objectContaining({ path: `result.${field}` }),
        }),
      );
    },
  );

  it("refuses an enrollment writer key that is not an address", () => {
    expect(() =>
      decodeRingRpcHealth({
        mode: "local",
        servicePubkey: KEY,
        enrollmentWriterPubkey: "writer",
        ...SWITCHES,
      }),
    ).toThrow(
      expect.objectContaining({
        details: expect.objectContaining({ path: "result.enrollmentWriterPubkey" }),
      }),
    );
  });
});
