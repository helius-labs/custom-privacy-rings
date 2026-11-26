import { ed25519 } from "@noble/curves/ed25519.js";
import { describe, expect, it } from "vitest";

import { initializePoseidon } from "@heliuslabs/spr";
import { rightAlign, u32be } from "@heliuslabs/spr/internal";
import type { Bytes32 } from "@heliuslabs/spr/interface";
import { poseidon, SigningKey } from "@heliuslabs/spr/keypair";

import { signingKeySpendSigner } from "../../src/operations/payments/anonymous.js";
import { signVouch, vouchDigest } from "../../src/proof/vouch.js";
import { ringError } from "../helpers/ring-pipeline.js";

await initializePoseidon();

const field = (byte: number): Bytes32 => {
  const value = new Uint8Array(32);
  value[31] = byte;
  return value as Bytes32;
};
const message = { ringId: field(5), privateTxHash: field(3), withdrawalHash: field(9) };

describe("the vouch digest", () => {
  it("hashes the CRVC tag, the ring id, the private tx hash and the withdrawal hash in that order", () => {
    const tag = rightAlign(u32be(Buffer.from("CRVC", "ascii").readUInt32BE(0)));
    const digest = vouchDigest(message);
    expect(digest).toEqual(poseidon([tag, field(5), field(3), field(9)]));
    expect(digest).not.toEqual(poseidon([tag, field(5), field(9), field(3)]));
    expect(digest).not.toEqual(poseidon([tag, field(3), field(9)]));
    expect(message).toEqual({
      ringId: field(5),
      privateTxHash: field(3),
      withdrawalHash: field(9),
    });
  });
});

describe("signVouch", () => {
  it("signs the digest's 32 bytes under the voucher's key", async () => {
    const key = SigningKey.generate();
    const signed = await signVouch(signingKeySpendSigner(key), message);
    expect(signed.publicKey).toEqual(key.publicKey().ed25519());
    expect(ed25519.verify(signed.signature, vouchDigest(message), signed.publicKey)).toBe(true);
    expect(
      ed25519.verify(
        signed.signature,
        vouchDigest({ ...message, withdrawalHash: field(8) }),
        signed.publicKey,
      ),
    ).toBe(false);
    expect(
      ed25519.verify(
        signed.signature,
        vouchDigest({ ...message, ringId: field(6) }),
        signed.publicKey,
      ),
    ).toBe(false);
  });

  it("refuses a signature the voucher's key did not make over the digest", async () => {
    const key = SigningKey.generate();
    const other = signingKeySpendSigner(SigningKey.generate());
    const wrongKey = { publicKey: () => key.publicKey(), sign: other.sign.bind(other) };
    await expect(signVouch(wrongKey, message)).rejects.toThrow(
      ringError("RING_VOUCHER_SIGNATURE_MISMATCH"),
    );
  });

  it("signs under a fresh throwaway key when nobody vouches", async () => {
    const [first, second] = await Promise.all([
      signVouch(undefined, message),
      signVouch(undefined, message),
    ]);
    expect(first.publicKey).not.toEqual(second.publicKey);
    for (const signed of [first, second])
      expect(ed25519.verify(signed.signature, vouchDigest(message), signed.publicKey)).toBe(true);
  });
});
