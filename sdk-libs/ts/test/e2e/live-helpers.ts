import { address } from "@solana/kit";
import type { ProofDataSource } from "@heliuslabs/spr/client";
import { createCustomRingClient } from "../../src/rpc/client.js";

export {
  currentSlot,
  signSendAndConfirm,
  waitForSignature,
  type Actor,
} from "../../../../zolana/sdk-libs/ts/test/e2e/live-helpers.js";

export function requiredEnv(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`the ring live test requires ${name}`);
  return value;
}

export function liveClient(proofDataSource?: ProofDataSource) {
  return createCustomRingClient({
    solanaRpcUrl: requiredEnv("ZOLANA_LOCALNET_URL"),
    indexerUrl: requiredEnv("ZOLANA_INDEXER_URL"),
    proverUrl: requiredEnv("ZOLANA_PROVER_URL"),
    tree: address(requiredEnv("ZOLANA_TREE")),
    ...(proofDataSource === undefined ? {} : { proofDataSource }),
    indexerConfig: { poll: { numRetries: 120, delayMs: 250n, maxDelayMs: 2_000n } },
  });
}
