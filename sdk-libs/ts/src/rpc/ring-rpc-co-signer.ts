import { ed25519 } from "@noble/curves/ed25519.js";
import {
  getBase58Encoder,
  getBase64Decoder,
  type SignatureBytes,
  type TransactionPartialSigner,
} from "@solana/kit";

import type { Address } from "@heliuslabs/spr/interface";
import { addressBytes, type indexerTypes } from "@heliuslabs/spr/internal";

import { RingError } from "../protocol/error.js";
import { RingRpcMethod, type ApprovedTransfer } from "../protocol/ring-rpc/ring-rpc-types.js";
import type { RingRpc } from "./ring-rpc.js";

const base58Encoder = getBase58Encoder();
const base64Decoder = getBase64Decoder();

export interface RingRpcCoSignerInput {
  readonly rpc: Pick<RingRpc, "coSignTransaction">;
  readonly ring: Address;
  /** The co-signer key the caller pinned, never one the service reports. */
  readonly signer: Address;
  /** `ProvenRingTransfer.approvedTransfer` of the transfer a program calls under the transaction's `ApproveTransfer`. */
  readonly approvedTransfer?: ApprovedTransfer;
}

/** Ring RPC as a co-signer of a transaction the wallet sends, refused unless `signer` signed the message. */
export function ringRpcCoSigner(input: RingRpcCoSignerInput): TransactionPartialSigner {
  const { rpc, ring, signer } = input;
  const publicKey = addressBytes(signer, "signer");
  const approvedTransfer =
    input.approvedTransfer === undefined ? {} : { approvedTransfer: input.approvedTransfer };
  return Object.freeze({
    address: signer,
    signTransactions: (transactions, config) =>
      Promise.all(
        transactions.map(async (transaction) => {
          const message = new Uint8Array(transaction.messageBytes);
          const answer = await rpc.coSignTransaction(
            {
              ring,
              message: base64Decoder.decode(message) as indexerTypes.Base64String,
              ...approvedTransfer,
            },
            config?.abortSignal === undefined ? undefined : { signal: config.abortSignal },
          );
          if (answer.signer !== signer) throw refused("signer is not the expected co-signer");
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
}

function refused(reason: string): RingError {
  return new RingError("RING_RPC", {
    details: { method: RingRpcMethod.coSignTransaction, reason },
  });
}
