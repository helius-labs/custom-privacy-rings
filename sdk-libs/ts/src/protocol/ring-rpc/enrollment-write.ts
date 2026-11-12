/** Rust `EnrollmentWrite` and `SignedEnrollment`, the write an owner signs for Ring RPC, the sole enrollment writer. */
import { addressBytes, encodeBase58, Writer } from "@heliuslabs/spr/internal";
import type { Address, Bytes32, Bytes64 } from "@heliuslabs/spr/interface";

import { checkedCustomRingProof } from "../codec/ring-codecs.js";
import { checkedViewingU } from "../membership/member-opening.js";
import { memberOfTag } from "../membership/policy.js";
import {
  trackingEntryHash,
  trackingEntryOf,
  trackingRotationOf,
  type TrackingEntry,
} from "../membership/tracking.js";

/** Rust `ENROLLMENT_BATCH`, the most writes one `EnrollBatch` carries. */
export const ENROLLMENT_BATCH = 5;
/** Rust `domain::ENROLLMENT_SIGNATURE_DOMAIN`, the first line of the text an owner signs. */
export const ENROLLMENT_SIGNATURE_DOMAIN = "zolana/ring-enrollment";
/** Rust `SIGNED_MESSAGE_MAX_LEN`, the text with each base58 value at its widest. */
export const SIGNED_MESSAGE_MAX_LEN = 144;

/** Rust `enrollment_kind`. */
export const EnrollmentKind = Object.freeze({ enroll: 0, rotate: 1 } as const);

/** Rust `EnrollIxData`. */
export interface EnrollmentRecord {
  readonly nullifierPk: Bytes32;
  readonly listTag: Bytes32;
  readonly viewingU: Bytes32;
  readonly ephU: Bytes32;
  readonly ciphertext: Bytes64;
}

/** Rust `EnrollmentChange`. */
export type EnrollmentChange =
  | Readonly<{ kind: "enroll"; record: EnrollmentRecord; proof: Uint8Array }>
  | Readonly<{ kind: "rotate"; viewingU: Bytes32; priorViewingU: Bytes32; priorSeq: bigint }>;

/** Rust `EnrollmentWrite`. */
export interface EnrollmentWrite {
  readonly owner: Address;
  readonly change: EnrollmentChange;
}

/** Rust `SignedEnrollment`, a write for the tracking registry `ring` pins. */
export interface SignedEnrollment {
  readonly ring: Address;
  readonly registry: Address;
  readonly write: EnrollmentWrite;
}

/** Rust `EnrollmentWrite::encode_into`, `owner || kind || body`. */
export function encodeEnrollmentWrite(write: EnrollmentWrite): Uint8Array {
  const writer = new Writer().bytes(addressBytes(write.owner, "owner"), 32, "owner");
  const { change } = write;
  if (change.kind === "rotate")
    return writer
      .u8(EnrollmentKind.rotate, "kind")
      .bytes(checkedViewingU(change.viewingU), 32, "viewingU")
      .bytes(change.priorViewingU, 32, "priorViewingU")
      .u64(change.priorSeq, "priorSeq")
      .finish();
  return writer
    .u8(EnrollmentKind.enroll, "kind")
    .bytes(encodeEnrollmentRecord(change.record), 192, "record")
    .bytes(checkedCustomRingProof(change.proof), 192, "proof")
    .finish();
}

/** Rust `EnrollmentWrite::entry`. */
export function enrollmentWriteEntry(write: EnrollmentWrite): TrackingEntry {
  const member = memberOfTag(addressBytes(write.owner, "owner"));
  const { change } = write;
  return change.kind === "rotate"
    ? trackingRotationOf({
        member,
        viewingU: checkedViewingU(change.viewingU),
        priorViewingU: change.priorViewingU,
        priorSeq: change.priorSeq,
      })
    : trackingEntryOf({ member, ...change.record });
}

/** Rust `SignedWrite`, the chain and the signature check read the same entry hash. */
export interface SignedWrite {
  readonly entryHash: Bytes32;
  readonly message: Uint8Array;
}

/**
 * Rust `EnrollmentWrite::signed`, text that binds one registry and every entry field, the `RegisterKey`
 * proof alone binds `eph_u`. The owner is the signing key, and the entry hash binds it too.
 */
export function signedWrite(write: EnrollmentWrite, registry: Address): SignedWrite {
  const entryHash = trackingEntryHash(enrollmentWriteEntry(write));
  const lines = [
    ENROLLMENT_SIGNATURE_DOMAIN,
    `registry: ${encodeBase58(addressBytes(registry, "registry"))}`,
    `entry: ${encodeBase58(entryHash)}`,
    `action: ${write.change.kind}`,
  ];
  return Object.freeze({ entryHash, message: new TextEncoder().encode(lines.join("\n")) });
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
