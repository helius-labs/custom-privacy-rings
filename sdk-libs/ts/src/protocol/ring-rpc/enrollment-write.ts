/** Rust `EnrollmentWrite` and `SignedEnrollment`, the write an owner signs for Ring RPC, the sole enrollment writer. */
import { addressBytes, concatBytes, Writer } from "@heliuslabs/spr/internal";
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
/** Rust `domain::ENROLLMENT_SIGNATURE_LABEL`. */
export const ENROLLMENT_SIGNATURE_LABEL = new TextEncoder().encode("CRING/enr");
/** Rust `SIGNED_MESSAGE_LEN`. */
export const SIGNED_MESSAGE_LEN = 105;

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
  | Readonly<{ kind: "rotate"; viewingU: Bytes32; priorViewingHash: Bytes32 }>;

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
      .bytes(change.priorViewingHash, 32, "priorViewingHash")
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
        priorViewingHash: change.priorViewingHash,
      })
    : trackingEntryOf({ member, ...change.record });
}

/** Rust `SignedWrite`, the chain and the signature check read the same entry hash. */
export interface SignedWrite {
  readonly entryHash: Bytes32;
  readonly message: Uint8Array;
}

/** Rust `EnrollmentWrite::signed`, binds one registry and every entry field, the `RegisterKey` proof alone binds `eph_u`. */
export function signedWrite(write: EnrollmentWrite, registry: Address): SignedWrite {
  const entryHash = trackingEntryHash(enrollmentWriteEntry(write));
  return Object.freeze({
    entryHash,
    message: concatBytes(
      ENROLLMENT_SIGNATURE_LABEL,
      addressBytes(registry, "registry"),
      addressBytes(write.owner, "owner"),
      entryHash,
    ),
  });
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
