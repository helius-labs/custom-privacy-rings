/**
 * The owner's signature over an enrollment write, Rust `SignedEnrollment::sign`. The owner signs
 * no transaction, Ring RPC batches the signed write under the ring's `enrollmentWriter` role and
 * storage checks the signature through the ed25519 precompile. A PDA holds no key and cannot enroll.
 *
 * Read after `enrollment.ts` and `viewing-rotation.ts`, the two modules that build the write.
 */
import { getBase58Decoder, getBase64Decoder, type MessagePartialSigner } from "@solana/kit";
import type { Signature } from "@heliuslabs/spr/interface";
import type { indexerTypes } from "@heliuslabs/spr/internal";

import {
  encodeEnrollmentWrite,
  signedWrite,
  type SignedEnrollment,
} from "../../protocol/ring-rpc/enrollment-write.js";
import type { SubmitEnrollmentRequest } from "../../protocol/ring-rpc/ring-rpc-types.js";
import { mismatch } from "../../rpc/relay-message.js";
import { signMessage } from "../../rpc/ring-rpc.js";

/** The request `RingRpc.submitEnrollment` sends, signed by the write's owner. */
export async function signEnrollment(
  signed: SignedEnrollment,
  owner: MessagePartialSigner,
): Promise<SubmitEnrollmentRequest> {
  if (owner.address !== signed.write.owner) throw mismatch("Signer");
  const signature = await signMessage(owner, signedWrite(signed.write, signed.registry).message);
  return Object.freeze({
    ring: signed.ring,
    write: getBase64Decoder().decode(
      encodeEnrollmentWrite(signed.write),
    ) as indexerTypes.Base64String,
    signature: getBase58Decoder().decode(signature) as Signature,
  });
}
