import { addressBytes, encodeBase58 } from "@heliuslabs/spr/internal";
import {
  createKeyPairSignerFromPrivateKeyBytes,
  getBase58Encoder,
  getBase64Encoder,
  verifySignature,
  type Address,
} from "@solana/kit";
import { describe, expect, it, vi } from "vitest";

import type { EnrollmentSeal } from "../../../src/operations/management/enrollment.js";
import {
  prepareEnrollmentRelay,
  prepareEnrollRelay,
} from "../../../src/operations/management/enroll-relay.js";
import {
  signedWrite,
  type RelayEnrollInputs,
} from "../../../src/protocol/ring-rpc/relay-enroll-inputs.js";
import { PendingRelaySignature, type RingRelayClient } from "../../../src/rpc/relay.js";
import { filled, ringFixture } from "../../helpers/ring-pipeline.js";

const address = (byte: number): Address => encodeBase58(new Uint8Array(32).fill(byte)) as Address;
const FEE_PAYER = address(0x44);
const REGISTRY = address(0x55);
const quote = { feePayer: FEE_PAYER, validUntilSlot: 100n };
const SEAL: EnrollmentSeal = {
  nullifierPk: filled(1),
  listTag: filled(2),
  ephU: filled(3),
  ciphertext: new Uint8Array(64).fill(4) as EnrollmentSeal["ciphertext"],
};
const ownerSigner = () => createKeyPairSignerFromPrivateKeyBytes(new Uint8Array(32).fill(1));

function inputsOf(owner: Address): RelayEnrollInputs {
  return {
    ringProgramId: address(0x90),
    trackingRegistry: REGISTRY,
    write: { owner, change: { kind: "rotate", viewingU: filled(5), priorViewingHash: filled(6) } },
  };
}

function relayAnswering(step: Readonly<Record<string, unknown>>) {
  return {
    getRelayQuote: vi.fn(),
    getRelayStatus: vi.fn(),
    prepareRelay: vi.fn(async () => step),
    submitRelay: vi.fn(),
  } as unknown as RingRelayClient;
}

describe("relayed enrollment", () => {
  it("signs the write's message alone and takes Ring RPC's batch without a second signature", async () => {
    const owner = await ownerSigner();
    const inputs = inputsOf(owner.address);
    const prepared = await prepareEnrollRelay({ inputs, owner, quote });
    const { intent } = prepared.request;
    if (intent.operation !== "enroll") throw new Error("an enroll relay");
    expect(
      await verifySignature(
        owner.keyPair.publicKey,
        new Uint8Array(getBase58Encoder().encode(intent.ownerSignature)) as never,
        signedWrite(inputs.write, inputs.trackingRegistry).message,
      ),
    ).toBe(true);
    const relay = relayAnswering({ step: "submitted", relayId: prepared.relayId() });
    const sent = await prepared.send(relay);
    expect(sent).not.toBeInstanceOf(PendingRelaySignature);
    expect(relay.submitRelay).not.toHaveBeenCalled();
  });

  it("refuses an owner that is not the signer", async () => {
    const owner = await ownerSigner();
    await expect(
      prepareEnrollRelay({ inputs: inputsOf(address(9)), owner, quote }),
    ).rejects.toMatchObject({
      code: "RING_RELAY_MESSAGE_INVALID",
      details: { mismatch: "Signer" },
    });
  });

  it("carries a built enrollment to the relay under the ring's registry", async () => {
    const owner = await ownerSigner();
    const fixture = await ringFixture();
    const prepared = await prepareEnrollmentRelay(
      {
        enroll: {
          ringProgramId: fixture.ringProgramId,
          owner: owner.address,
          seal: SEAL,
          viewingU: filled(5),
          proof: new Uint8Array(192).fill(6),
        },
        owner,
        quote,
      },
      fixture.client,
    );
    expect(prepared.request.ring).toBe(fixture.ringProgramId);
    const { intent } = prepared.request;
    if (intent.operation !== "enroll") throw new Error("an enroll relay");
    const wire = new Uint8Array(getBase64Encoder().encode(intent.proofInputs));
    expect(wire.subarray(32, 64)).toEqual(addressBytes(fixture.trackingRegistry));
    expect(wire.subarray(64, 96)).toEqual(addressBytes(owner.address));
  });
});
