/** Rust `RelayEnrollInputs` in wincode. */
import {
  addressBytes,
  concatBytes,
  instruction,
  meta,
  sha256,
  Writer,
  type SignerAccount,
} from "@heliuslabs/spr/internal";
import type { Address, Bytes32 } from "@heliuslabs/spr/interface";
import type { AccountMeta, Instruction } from "@solana/kit";

import { enrollmentQueuePda, STORAGE_PROGRAM_ID } from "../pda/index.js";

export interface RelayEnrollInputs {
  readonly ringProgramId: Address;
  readonly owner: Address;
  readonly trackingRegistry: Address;
  /** Tag 6 first, the program holds every field to the owner's signature and proof. */
  readonly instructionData: Uint8Array;
}

/** The ceiling a relayed enrollment asks and its owner checks. */
export const ENROLL_COMPUTE_UNITS = 400_000;

const ENROLL_REQUEST_DOMAIN = new TextEncoder().encode("zolana/ring-rpc-relay-enroll");

export function encodeRelayEnrollInputs(value: RelayEnrollInputs): Uint8Array {
  return new Writer()
    .bytes(addressBytes(value.ringProgramId), 32, "ringProgramId")
    .bytes(addressBytes(value.owner), 32, "owner")
    .bytes(addressBytes(value.trackingRegistry), 32, "trackingRegistry")
    .u64(BigInt(value.instructionData.length), "instructionData.length")
    .bytes(value.instructionData, undefined, "instructionData")
    .finish();
}

/** The owner's signature over it spends the relay's budget, it names no quote so a retry keeps it. */
export function enrollRequestDigest(value: RelayEnrollInputs): Bytes32 {
  return sha256(
    concatBytes(
      ENROLL_REQUEST_DOMAIN,
      addressBytes(value.ringProgramId),
      addressBytes(value.owner),
      addressBytes(value.trackingRegistry),
      value.instructionData,
    ),
  ) as Bytes32;
}

/** Rust `Enroll` accounts, the owner signs and the queue alone is written. */
export async function enrollAccounts(
  input: Readonly<{ owner: SignerAccount; trackingRegistry: Address }>,
): Promise<readonly AccountMeta[]> {
  const [queue] = await enrollmentQueuePda(input.trackingRegistry);
  return [
    meta(input.owner, true, false),
    meta(input.trackingRegistry, false, false),
    meta(queue, false, true),
  ];
}

/** The relay pays the fee as the message's fee payer, the instruction names no payer. */
export async function relayEnrollInstruction(value: RelayEnrollInputs): Promise<Instruction> {
  return instruction(value.instructionData, await enrollAccounts(value), STORAGE_PROGRAM_ID);
}
