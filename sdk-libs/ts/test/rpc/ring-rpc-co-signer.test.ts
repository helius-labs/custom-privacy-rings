import { ed25519 } from "@noble/curves/ed25519.js";
import { getBase58Decoder, getBase64Encoder, type TransactionPartialSigner } from "@solana/kit";
import { describe, expect, it } from "vitest";

import type { Address, Signature } from "@heliuslabs/spr/interface";

import type {
  ApprovedTransfer,
  CoSignTransactionRequest,
  CoSignTransactionResponse,
} from "../../src/protocol/ring-rpc/ring-rpc-types.js";
import { ringRpcCoSigner } from "../../src/rpc/ring-rpc-co-signer.js";

const base58 = getBase58Decoder();

const secretKey = new Uint8Array(32).fill(8);
const signer = base58.decode(ed25519.getPublicKey(secretKey)) as Address;
const ring = base58.decode(new Uint8Array(32).fill(7)) as Address;
const payer = base58.decode(new Uint8Array(32).fill(6)) as Address;
/** A v1 message names its signer count in its second byte, the fee payer and the co-signer. */
const messageBytes = Uint8Array.of(0x81, 2, 2, 3, 4);
const payerSignature = new Uint8Array(64).fill(5);

type Transaction = Parameters<TransactionPartialSigner["signTransactions"]>[0][number];
const transaction = {
  messageBytes,
  signatures: { [payer]: payerSignature, [signer]: null },
} as unknown as Transaction;
const wire = Buffer.from([...messageBytes, ...payerSignature, ...new Uint8Array(64)]).toString(
  "base64",
);

function answering(
  answer: (message: Uint8Array) => CoSignTransactionResponse,
  approvedTransfer?: ApprovedTransfer,
) {
  const requests: CoSignTransactionRequest[] = [];
  const rpc = {
    coSignTransaction: async (request: CoSignTransactionRequest) => {
      requests.push(request);
      const sent = new Uint8Array(getBase64Encoder().encode(request.transaction));
      return answer(sent.subarray(0, messageBytes.length));
    },
  };
  return {
    requests,
    coSigner: ringRpcCoSigner({
      rpc,
      ring,
      signer,
      ...(approvedTransfer === undefined ? {} : { approvedTransfer }),
    }),
  };
}

function signedBy(key: Uint8Array, message: Uint8Array): Signature {
  return base58.decode(ed25519.sign(message, key)) as Signature;
}

describe("ringRpcCoSigner", () => {
  it("sends the transaction with every other signature and returns the service's signature over the message bytes", async () => {
    const { requests, coSigner } = answering((message) => ({
      signer,
      signature: signedBy(secretKey, message),
    }));
    expect(coSigner.address).toBe(signer);
    const [signatures] = await coSigner.signTransactions([transaction]);
    expect(signatures?.[signer]).toEqual(ed25519.sign(messageBytes, secretKey));
    expect(requests).toEqual([{ ring, transaction: wire }]);
  });

  it("refuses a transaction another required signer has not signed before any call", async () => {
    const { requests, coSigner } = answering((message) => ({
      signer,
      signature: signedBy(secretKey, message),
    }));
    const unsigned = {
      messageBytes,
      signatures: { [payer]: null, [signer]: null },
    } as unknown as Transaction;
    await expect(coSigner.signTransactions([unsigned])).rejects.toMatchObject({
      code: "RING_RPC",
      details: { method: "coSignTransaction", reason: "a required signature is missing" },
    });
    expect(requests).toEqual([]);
  });

  it("names the transfer a program calls beside the message", async () => {
    const approvedTransfer = {
      data: "NwcI" as ApprovedTransfer["data"],
      accounts: [ring],
    };
    const { requests, coSigner } = answering(
      (message) => ({ signer, signature: signedBy(secretKey, message) }),
      approvedTransfer,
    );
    await coSigner.signTransactions([transaction]);
    expect(requests).toEqual([{ ring, transaction: wire, approvedTransfer }]);
  });

  it("refuses a signature from another signer", async () => {
    const otherKey = new Uint8Array(32).fill(9);
    const { coSigner } = answering((message) => ({
      signer: base58.decode(ed25519.getPublicKey(otherKey)) as Address,
      signature: signedBy(otherKey, message),
    }));
    await expect(coSigner.signTransactions([transaction])).rejects.toMatchObject({
      code: "RING_RPC",
      details: { method: "coSignTransaction", reason: "signer is not the expected co-signer" },
    });
  });

  it("refuses a signature over other bytes", async () => {
    const { coSigner } = answering(() => ({
      signer,
      signature: signedBy(secretKey, Uint8Array.of(9, 9, 9)),
    }));
    await expect(coSigner.signTransactions([transaction])).rejects.toMatchObject({
      code: "RING_RPC",
      details: { method: "coSignTransaction", reason: "co-signature is invalid" },
    });
  });
});
