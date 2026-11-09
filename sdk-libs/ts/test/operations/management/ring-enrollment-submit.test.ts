import { encodeBase58 } from "@heliuslabs/spr/internal";
import {
  createKeyPairSignerFromPrivateKeyBytes,
  getBase58Encoder,
  getBase64Encoder,
  verifySignature,
  type Address,
} from "@solana/kit";
import { beforeAll, describe, expect, it, vi } from "vitest";

import { initializePoseidon } from "@heliuslabs/spr";

import { signEnrollment } from "../../../src/operations/management/enrollment-submit.js";
import { encodeSubmitEnrollmentRequest } from "../../../src/protocol/ring-rpc/ring-rpc-codec.js";
import {
  encodeEnrollmentWrite,
  signedWrite,
  type SignedEnrollment,
} from "../../../src/protocol/ring-rpc/enrollment-write.js";
import { RingRpc } from "../../../src/rpc/ring-rpc.js";
import { filled } from "../../helpers/ring-pipeline.js";

const address = (byte: number): Address => encodeBase58(new Uint8Array(32).fill(byte)) as Address;
const RING = address(0x90);
const REGISTRY = address(0x55);
const ownerSigner = () => createKeyPairSignerFromPrivateKeyBytes(new Uint8Array(32).fill(1));

function rotationOf(owner: Address): SignedEnrollment {
  return {
    ring: RING,
    registry: REGISTRY,
    write: { owner, change: { kind: "rotate", viewingU: filled(5), priorViewingHash: filled(6) } },
  };
}

beforeAll(initializePoseidon);

describe("enrollment submission", () => {
  it("signs the write's message with the owner's key and carries the write's bytes", async () => {
    const owner = await ownerSigner();
    const signed = rotationOf(owner.address);
    const request = await signEnrollment(signed, owner);
    expect(request.ring).toBe(RING);
    expect(new Uint8Array(getBase64Encoder().encode(request.write))).toEqual(
      encodeEnrollmentWrite(signed.write),
    );
    expect(
      await verifySignature(
        owner.keyPair.publicKey,
        new Uint8Array(getBase58Encoder().encode(request.signature)) as never,
        signedWrite(signed.write, REGISTRY).message,
      ),
    ).toBe(true);
    expect(Object.keys(encodeSubmitEnrollmentRequest(request))).toEqual([
      "ring",
      "write",
      "signature",
    ]);
  });

  it("refuses a signer other than the write's owner", async () => {
    const owner = await ownerSigner();
    await expect(signEnrollment(rotationOf(address(9)), owner)).rejects.toMatchObject({
      code: "RING_RELAY_MESSAGE_INVALID",
      details: { mismatch: "Signer" },
    });
  });

  it("posts submitEnrollment and accepts only an empty answer", async () => {
    const owner = await ownerSigner();
    const request = await signEnrollment(rotationOf(owner.address), owner);
    const answer = (result: unknown) =>
      vi.fn(async (_url: string, init: RequestInit) => {
        const body = JSON.parse(String(init.body)) as { id: unknown; method: string };
        expect(body.method).toBe("submitEnrollment");
        return new Response(JSON.stringify({ jsonrpc: "2.0", id: body.id, result }), {
          headers: { "content-type": "application/json" },
        });
      });
    const ok = new RingRpc("https://ring-rpc.test", { fetch: answer({}) as never });
    await expect(ok.submitEnrollment(request)).resolves.toBeUndefined();
    const extra = new RingRpc("https://ring-rpc.test", { fetch: answer({ held: 1 }) as never });
    await expect(extra.submitEnrollment(request)).rejects.toThrow();
  });
});
