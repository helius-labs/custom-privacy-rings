/** Rust `RelayEnrollInputs` in wincode, the enrollment request Ring RPC alone writes on chain. */
import { addressBytes, concatBytes, sha256, Writer } from "@heliuslabs/spr/internal";
import type { Address, Bytes32, Bytes64 } from "@heliuslabs/spr/interface";

import { checkedCustomRingProof } from "../codec/ring-codecs.js";
import { checkedViewingU } from "../membership/member-opening.js";

/** Rust `EnrollIxData`, the record the tracking leaf takes its fields from. */
export interface EnrollmentRecord {
  readonly nullifierPk: Bytes32;
  readonly listTag: Bytes32;
  readonly viewingU: Bytes32;
  readonly ephU: Bytes32;
  readonly ciphertext: Bytes64;
}

export interface RelayEnrollInputs {
  readonly ringProgramId: Address;
  readonly owner: Address;
  readonly trackingRegistry: Address;
  readonly record: EnrollmentRecord;
  /** The `RegisterKey` proof on a first enrollment, none on a rotation. */
  readonly proof?: Uint8Array;
}

const ENROLL_REQUEST_DOMAIN = new TextEncoder().encode("zolana/ring-rpc-relay-enroll");

export function encodeRelayEnrollInputs(value: RelayEnrollInputs): Uint8Array {
  const writer = new Writer()
    .bytes(addressBytes(value.ringProgramId), 32, "ringProgramId")
    .bytes(addressBytes(value.owner), 32, "owner")
    .bytes(addressBytes(value.trackingRegistry), 32, "trackingRegistry")
    .bytes(encodeEnrollmentRecord(value.record), 192, "record")
    .u8(value.proof === undefined ? 0 : 1, "proof.present");
  if (value.proof !== undefined)
    writer.bytes(checkedCustomRingProof(value.proof), value.proof.length, "proof");
  return writer.finish();
}

/**
 * The owner signs it over the domain, the ring, the owner, the registry, the record and the proof,
 * so Ring RPC can write no other enrollment for the owner. It names no quote, a retry keeps it.
 */
export function enrollRequestDigest(value: RelayEnrollInputs): Bytes32 {
  return sha256(
    concatBytes(
      ENROLL_REQUEST_DOMAIN,
      addressBytes(value.ringProgramId),
      addressBytes(value.owner),
      addressBytes(value.trackingRegistry),
      encodeEnrollmentRecord(value.record),
      value.proof ?? new Uint8Array(),
    ),
  ) as Bytes32;
}

/** `nullifier_pk || list_tag || viewing_u || eph_u || ciphertext`, a noncanonical viewing key refused. */
export function encodeEnrollmentRecord(record: EnrollmentRecord): Uint8Array {
  return new Writer()
    .bytes(record.nullifierPk, 32, "nullifierPk")
    .bytes(record.listTag, 32, "listTag")
    .bytes(checkedViewingU(record.viewingU), 32, "viewingU")
    .bytes(record.ephU, 32, "ephU")
    .bytes(record.ciphertext, 64, "ciphertext")
    .finish();
}
