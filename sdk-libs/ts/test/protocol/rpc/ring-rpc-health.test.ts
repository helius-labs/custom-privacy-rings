import { describe, expect, it } from "vitest";

import { decodeRingRpcHealth } from "../../../src/protocol/ring-rpc/health.js";

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
        coSignerPubkey: null,
        enrollmentWriterPubkey: WRITER,
        ...SWITCHES,
        newerField: 1,
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

  it.each(["relay", "recordWriter", "enrollmentWrites", "readerGrants"])(
    "refuses a result without %s",
    (field) => {
      const wire: Record<string, unknown> = { mode: "local", servicePubkey: KEY, ...SWITCHES };
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
