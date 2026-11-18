import { ed25519 } from "@noble/curves/ed25519.js";
import {
  address,
  getAddressEncoder,
  type MessagePartialSigner,
  type ReadonlyUint8Array,
  type SignatureBytes,
  type TransactionPartialSigner,
} from "@solana/kit";

import type { CustomRingClient } from "../../src/rpc/client.js";
import { requiredEnv } from "./live-helpers.js";

export type LocalnetSigner = MessagePartialSigner & TransactionPartialSigner;

export function authoritySigner(): LocalnetSigner {
  return localnetSigner(JSON.parse(requiredEnv("RING_AUTHORITY_SIGNER")));
}

export async function reservedProgram(client: CustomRingClient): Promise<LocalnetSigner> {
  const programs: unknown = JSON.parse(requiredEnv("RING_E2E_PROGRAM_SIGNERS"));
  if (!Array.isArray(programs)) throw new Error("expected an array of reserved program signers");
  for (const descriptor of programs) {
    const program = localnetSigner(descriptor);
    const { value } = await client.solanaRpc
      .getAccountInfo(program.address, { encoding: "base64" })
      .send();
    if (value === null) return program;
  }
  throw new Error("every reserved ring program is deployed");
}

function localnetSigner(descriptor: unknown): LocalnetSigner {
  if (
    typeof descriptor !== "object" ||
    descriptor === null ||
    !("address" in descriptor) ||
    typeof descriptor.address !== "string" ||
    !("key_arn" in descriptor) ||
    typeof descriptor.key_arn !== "string"
  )
    throw new Error("invalid localnet KMS signer descriptor");
  const signer = address(descriptor.address);
  const publicKey = new Uint8Array(getAddressEncoder().encode(signer));
  const endpoint = requiredEnv("RING_LOCALNET_KMS");
  if (!/^127\.0\.0\.1:\d+$/.test(endpoint)) throw new Error("expected a loopback KMS endpoint");
  const keyArn = descriptor.key_arn;
  const algorithm = "ED25519_SHA_512";
  async function sign(message: ReadonlyUint8Array, abortSignal?: AbortSignal) {
    const bytes = new Uint8Array(message);
    const response = await fetch(`http://${endpoint}/`, {
      method: "POST",
      redirect: "error",
      headers: {
        "Content-Type": "application/x-amz-json-1.1",
        "X-Amz-Target": "TrentService.Sign",
      },
      body: JSON.stringify({
        KeyId: keyArn,
        SigningAlgorithm: algorithm,
        MessageType: "RAW",
        Message: Buffer.from(bytes).toString("base64"),
      }),
      signal: abortSignal ?? AbortSignal.timeout(10_000),
    });
    if (!response.ok) throw new Error(`localnet KMS Sign failed with HTTP ${response.status}`);
    const reply = await response.json();
    if (
      reply.KeyId !== keyArn ||
      reply.SigningAlgorithm !== algorithm ||
      typeof reply.Signature !== "string"
    )
      throw new Error("unexpected localnet KMS signing key or algorithm");
    const signature = new Uint8Array(Buffer.from(reply.Signature, "base64"));
    if (!ed25519.verify(signature, bytes, publicKey, { zip215: false }))
      throw new Error("invalid localnet KMS signature");
    return { [signer]: signature as SignatureBytes };
  }
  return {
    address: signer,
    signTransactions: (transactions, config) =>
      Promise.all(transactions.map(({ messageBytes }) => sign(messageBytes, config?.abortSignal))),
    signMessages: (messages, config) =>
      Promise.all(messages.map(({ content }) => sign(content, config?.abortSignal))),
  };
}
