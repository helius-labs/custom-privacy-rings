/** What every shared relay message vector carries besides its cases. */
import { decodeRingPolicy, type RingPolicy } from "../../src/protocol/codec/ring-codecs.js";
import type { RelayMessageMismatch } from "../../src/rpc/relay-message.js";

const MISMATCHES: Readonly<Record<RelayMessageMismatch, true>> = {
  Undecodable: true,
  NotV1: true,
  FeePayer: true,
  Budget: true,
  Instructions: true,
  Program: true,
  Data: true,
  Accounts: true,
  Recipients: true,
  TransactionKey: true,
  ApprovalMaskUnaccepted: true,
  MissingApprovalDigest: true,
  FalseApprovalOpening: true,
  ApprovalOpeningLayout: true,
  ZeroApprovalBlinder: true,
  ReusedApprovalBlinder: true,
  ApprovalOverLimit: true,
  UnopenedOutput: true,
  Legs: true,
  SpendsUnnamedNote: true,
  SpendsUnrequestedNote: true,
  Unrequested: true,
  Signer: true,
};

/** A verdict name the TS check cannot return would pass no case and hide a Rust refusal. */
export function knownVerdict(verdict: string): string {
  if (verdict !== "Accepted" && !Object.hasOwn(MISMATCHES, verdict))
    throw new Error(`unknown verdict ${verdict}`);
  return verdict;
}

/** The vector's `ringPolicy`, the hex of the RingPolicy account. */
export function vectorRingPolicy(vector: Readonly<{ ringPolicy: string }>): RingPolicy {
  return decodeRingPolicy(Uint8Array.from(Buffer.from(vector.ringPolicy, "hex")));
}
