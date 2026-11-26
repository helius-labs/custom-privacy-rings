import { describe, expect, it } from "vitest";

import { wirePrimitives } from "../../src/protocol/wire-primitives.js";

const { u64, count, decimalU64, hash, text } = wirePrimitives((path, fault) => {
  throw new Error(`${path} ${fault}`);
});

const SAFE = Number.MAX_SAFE_INTEGER;

describe("wire primitives", () => {
  it("takes each u64 in one spelling, a number through 2^53 - 1 and quoted digits past it", () => {
    expect(u64(0, "value")).toBe(0n);
    expect(u64(SAFE, "value")).toBe(BigInt(SAFE));
    expect(u64("9007199254740992", "value")).toBe(2n ** 53n);
    expect(u64("18446744073709551615", "value")).toBe(2n ** 64n - 1n);
    for (const refused of [
      -1,
      1.5,
      SAFE + 1,
      Number.NaN,
      "0",
      String(SAFE),
      "18446744073709551616",
      "-9007199254740993",
      "09007199254740993",
      "9007199254740993.0",
      "9.007199254740993e15",
      1n,
      null,
      undefined,
    ])
      expect(() => u64(refused, "value")).toThrow("value integer");
  });

  it("bounds a count and takes a stored u64 as decimal text alone", () => {
    expect(count(0xffff, "index", 0xffff)).toBe(0xffff);
    for (const refused of [0x1_0000, -1, 0.5, "1", 1n])
      expect(() => count(refused, "index", 0xffff)).toThrow("index integer");
    expect(decimalU64("0", "slot")).toBe(0n);
    expect(decimalU64("18446744073709551615", "slot")).toBe(2n ** 64n - 1n);
    for (const refused of [0, 1n, "", "01", "-1", "1e3", "18446744073709551616"])
      expect(() => decimalU64(refused, "slot")).toThrow("slot integer");
  });

  it("refuses text of another type and a hash off 32 bytes or its base58 spelling", () => {
    expect(text("", "name")).toBe("");
    expect(() => text(1, "name")).toThrow("name type");
    expect(hash("11111111111111111111111111111111", "root")).toEqual(new Uint8Array(32));
    for (const refused of ["1".repeat(31), "1".repeat(33), "0OIl", 7, null])
      expect(() => hash(refused, "root")).toThrow("root type");
  });
});
