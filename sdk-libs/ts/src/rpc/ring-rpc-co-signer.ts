import { ed25519 } from "@noble/curves/ed25519.js";
import {
  getBase58Encoder,
  getBase64EncodedWireTransaction,
  type SignatureBytes,
  type TransactionPartialSigner,
} from "@solana/kit";

import type { Address } from "@heliuslabs/spr/interface";
import { addressBytes, type indexerTypes } from "@heliuslabs/spr/internal";

import { RingError } from "../protocol/error.js";
import { RingRpcMethod, type ApprovedTransfer } from "../protocol/ring-rpc/ring-rpc-types.js";
import type { RingRpc } from "./ring-rpc.js";

const base58Encoder = getBase58Encoder();

/** Kit asks partial signers at once, a submission asks these after every other signer. */
const SIGNS_LAST = new WeakSet<TransactionPartialSigner>();

export interface RingRpcCoSignerInput {
  readonly rpc: Pick<RingRpc, "coSignTransaction">;
  readonly ring: Address;
  /** The co-signer key the caller pinned, never one the service reports. */
  readonly signer: Address;
  /** `ProvenRingTransfer.approvedTransfer` of the transfer a program calls under the transaction's `ApproveTransfer`. */
  readonly approvedTransfer?: ApprovedTransfer;
}

/** Ring RPC as the last signer of a transaction the wallet sends, refused unless `signer` signed the message. */
export function ringRpcCoSigner(input: RingRpcCoSignerInput): TransactionPartialSigner {
  const { rpc, ring, signer } = input;
  const publicKey = addressBytes(signer, "signer");
  const approvedTransfer =
    input.approvedTransfer === undefined ? {} : { approvedTransfer: input.approvedTransfer };
  const coSigner: TransactionPartialSigner = Object.freeze({
    address: signer,
    signTransactions: (transactions, config) =>
      Promise.all(
        transactions.map(async (transaction) => {
          // Ring RPC simulates and reserves only for a transaction every other signer signed.
          if (
            Object.entries(transaction.signatures).some(
              ([address, signature]) => address !== signer && signature === null,
            )
          )
            throw refused("a required signature is missing");
          const answer = await rpc.coSignTransaction(
            {
              ring,
              transaction: getBase64EncodedWireTransaction({
                ...transaction,
                signatures: { ...transaction.signatures, [signer]: null },
              }) as string as indexerTypes.Base64String,
              ...approvedTransfer,
            },
            config?.abortSignal === undefined ? undefined : { signal: config.abortSignal },
          );
          if (answer.signer !== signer) throw refused("signer is not the expected co-signer");
          const message = new Uint8Array(transaction.messageBytes);
          const signature = new Uint8Array(base58Encoder.encode(answer.signature));
          if (
            signature.length !== 64 ||
            !ed25519.verify(signature, message, publicKey, { zip215: false })
          )
            throw refused("co-signature is invalid");
          return Object.freeze({ [signer]: signature as SignatureBytes });
        }),
      ),
  });
  SIGNS_LAST.add(coSigner);
  return coSigner;
}

/** @internal */
export function signsLast(signer: TransactionPartialSigner): boolean {
  return SIGNS_LAST.has(signer);
}

function refused(reason: string): RingError {
  return new RingError("RING_RPC", {
    details: { method: RingRpcMethod.coSignTransaction, reason },
  });
}
