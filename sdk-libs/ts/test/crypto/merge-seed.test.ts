import { bytesToHex } from "@noble/hashes/utils.js";
import { beforeAll, describe, expect, it } from "vitest";
import { initializePoseidon, type Bytes32 } from "@heliuslabs/spr";
import { mergeDummyNullifier, mergeOutputBlinding } from "@heliuslabs/spr/internal";
import { NullifierKey, type Bytes31 } from "@heliuslabs/spr/keypair";

import vector from "../../../../test-vectors/merge/merge.json" with { type: "json" };
import {
  seededMergeDummyNullifier,
  seededMergeOutputBlinding,
} from "../../src/crypto/derivation/merge-seed.js";

function bytes(value: string): Bytes32 {
  return Uint8Array.from(Buffer.from(value.replace(/^0x/, ""), "hex")) as Bytes32;
}

describe("seeded merge derivations", () => {
  beforeAll(initializePoseidon);

  for (const testCase of vector.cases) {
    const seed = bytes(testCase.request.seed);
    const first = bytes(testCase.request.firstNullifier);
    const nullifierKey = (): NullifierKey => NullifierKey.fromSecret(seed.slice(1) as Bytes31);

    it(`${testCase.name} derives the output blinding of the shared vector`, () => {
      expect(seed[0]).toBe(0);
      expect(`0x${bytesToHex(seededMergeOutputBlinding(seed, first))}`).toBe(
        testCase.output_blinding,
      );
      expect(seededMergeOutputBlinding(seed, first)).toEqual(
        mergeOutputBlinding(nullifierKey(), first),
      );
    });

    it(`${testCase.name} derives the padding nullifiers of the seed's nullifier key`, () => {
      for (const slot of [0, 7, 255])
        expect(seededMergeDummyNullifier(seed, first, slot)).toEqual(
          mergeDummyNullifier(nullifierKey(), first, slot),
        );
    });
  }

  it("refuses a dummy slot past a u8", () => {
    const [testCase] = vector.cases;
    expect(() =>
      seededMergeDummyNullifier(
        bytes(testCase?.request.seed ?? ""),
        bytes(testCase?.request.firstNullifier ?? ""),
        256,
      ),
    ).toThrow(RangeError);
  });
});
