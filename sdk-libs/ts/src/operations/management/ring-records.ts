import {
  getBase58Encoder,
  getSignatureFromTransaction,
  sendTransactionWithoutConfirmingFactory,
  signTransactionWithSigners,
  type MessagePartialSigner,
  type SignatureBytes,
  type TransactionPartialSigner,
} from "@solana/kit";
import type {
  BlockhashProvider,
  ChainReader,
  KitRpcAccess,
  TransactionConfirmer,
} from "@heliuslabs/spr/client";
import type { Address, RequestContext } from "@heliuslabs/spr/interface";
import { runKitRpc } from "@heliuslabs/spr/internal";

import { RingError } from "../../protocol/error.js";
import { ringRecordsAddress } from "../../protocol/pda/index.js";
import type { RingRpc } from "../../rpc/ring-rpc/client.js";
import { signCreateRecordsRegistryRequest } from "../../rpc/ring-rpc/policy-authority.js";
import type { RingReadTarget } from "../../rpc/ring-rpc/signed-request.js";
import { recordsCreationTransaction } from "./admin.js";

const base58Encoder = getBase58Encoder();

export type RingRecordsClient = BlockhashProvider &
  KitRpcAccess &
  Pick<TransactionConfirmer, "confirmTransaction"> &
  Pick<ChainReader, "getAccount">;

/** Rust `ring init`'s records step, the ring authority pays and Ring RPC's storage authority creates. */
export async function createRingRecords(
  input: Readonly<{
    client: RingRecordsClient;
    ringRpc: Pick<RingRpc, "health" | "createRecordsRegistry">;
    target: RingReadTarget;
    authority: TransactionPartialSigner & MessagePartialSigner;
  }>,
  context?: RequestContext,
): Promise<Address> {
  const { client, ringRpc, target, authority } = input;
  // 1. The storage authority comes from the service the caller pinned, an existing registry needs no call.
  const { storageAuthorityPublicKey: storageAuthority } = await ringRpc.health(context);
  if (storageAuthority === undefined)
    throw new RingError("RING_RPC", {
      details: { method: "health", reason: "no storage authority" },
    });
  const registry = await ringRecordsAddress(target.ringProgramId, storageAuthority);
  if ((await client.getAccount(registry, context)) !== undefined) return registry;
  // 2. The service signs only the creation it rebuilds, `createRecordsRegistry` verifies its signature.
  const transaction = await recordsCreationTransaction({
    ringProgramId: target.ringProgramId,
    payer: authority.address,
    storageAuthority,
    lifetime: await client.getLatestBlockhash(context),
  });
  const created = await ringRpc.createRecordsRegistry(
    await signCreateRecordsRegistryRequest(
      target,
      authority,
      new Uint8Array(transaction.messageBytes),
    ),
    context,
  );
  if (created.signature === undefined) return registry;
  // 3. The payer signs last and sends.
  const signed = await signTransactionWithSigners(
    [authority],
    {
      ...transaction,
      signatures: {
        ...transaction.signatures,
        [storageAuthority]: base58Encoder.encode(created.signature) as SignatureBytes,
      },
    },
    context?.signal === undefined ? {} : { abortSignal: context.signal },
  );
  const send = sendTransactionWithoutConfirmingFactory({ rpc: client.solanaRpc });
  await runKitRpc("sendTransaction", context, (abortSignal) =>
    send(signed, { commitment: client.commitment, abortSignal }),
  );
  await client.confirmTransaction(getSignatureFromTransaction(signed), undefined, context);
  return registry;
}
