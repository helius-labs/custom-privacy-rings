import { getBase58Decoder, getBase64Decoder, type MessagePartialSigner } from "@solana/kit";
import type { RequestContext, Signature } from "@heliuslabs/spr/interface";

import {
  encodeRelayEnrollInputs,
  enrollRequestDigest,
  type RelayEnrollInputs,
} from "../../protocol/ring-rpc/relay-enroll-inputs.js";
import type { RelayIntent, RelayQuote } from "../../protocol/ring-rpc/ring-rpc-types.js";
import { mismatch } from "../../rpc/relay-message.js";
import { PreparedRelay } from "../../rpc/relay.js";
import { signMessage } from "../../rpc/ring-rpc.js";
import { enrollmentRequest, type Enroll, type EnrollmentReadClient } from "./enrollment.js";

/** The owner signs the request alone, Ring RPC writes the enrollment and asks no second signature. */
export async function prepareEnrollRelay(
  input: Readonly<{
    inputs: RelayEnrollInputs;
    owner: MessagePartialSigner;
    quote: RelayQuote;
  }>,
): Promise<PreparedRelay> {
  const { inputs, owner, quote } = input;
  if (owner.address !== inputs.owner) throw mismatch("Signer");
  const signature = await signMessage(owner, enrollRequestDigest(inputs));
  const intent: RelayIntent = {
    operation: "enroll",
    proofInputs: getBase64Decoder().decode(encodeRelayEnrollInputs(inputs)) as never,
    ownerSignature: getBase58Decoder().decode(signature) as Signature,
  };
  return new PreparedRelay({ ring: inputs.ringProgramId, quote, intent });
}

/** A first enrollment or a rotation, checked against the owner's leaf before the owner signs. */
export async function prepareEnrollmentRelay(
  input: Readonly<{
    enroll: Enroll;
    owner: MessagePartialSigner;
    quote: RelayQuote;
  }>,
  client: EnrollmentReadClient,
  context?: RequestContext,
): Promise<PreparedRelay> {
  return prepareEnrollRelay({
    inputs: await enrollmentRequest(input.enroll, client, context),
    owner: input.owner,
    quote: input.quote,
  });
}
