import { copyBytes, equalBytes, isZero, poseidon } from "@heliuslabs/spr/internal";
import type { Bytes32 } from "@heliuslabs/spr/interface";

import { domainField, RingDomain } from "../../crypto/derivation/ring-domain.js";
import { listTagOf } from "../../crypto/keys/list-salt.js";
import { RingError } from "../error.js";

/**
 * `Poseidon(CRPL, owner_pk_hash, 0)`, the key of an owner's public lists, Rust `Member::public`.
 * Registration refuses a zero salt, so no private record shares it.
 */
export function publicListKey(ownerPkHash: Bytes32): Bytes32 {
  return lookupKey(ownerPkHash, ZERO_SALT);
}

/** `Poseidon(CRPL, owner_pk_hash, list_salt)`, the key of a private entry, Rust `Member::salted`. */
export function privateListKey(ownerPkHash: Bytes32, listSalt: Bytes32): Bytes32 {
  const checkedOwner = copyBytes(ownerPkHash, 32, "ownerPkHash") as Bytes32;
  const checkedSalt = copyBytes(listSalt, 32, "listSalt") as Bytes32;
  if (isZero(checkedSalt)) throw refused("zeroSalt");
  return lookupKey(checkedOwner, checkedSalt);
}

/** `Poseidon(CRPR, owner_pk_hash, ring_salt)`, Rust `Member::ring_salted`. */
export function ringListKey(ownerPkHash: Bytes32, ringSalt: Bytes32): Bytes32 {
  return poseidon([
    domainField(RingDomain.ringListPosition),
    copyBytes(ownerPkHash, 32, "ownerPkHash"),
    copyBytes(ringSalt, 32, "ringSalt"),
  ]) as Bytes32;
}

/** Refused unless the salt opens the tag the policy commits. */
export function committedRingListKey(
  input: Readonly<{ ownerPkHash: Bytes32; ringSalt: Bytes32; ringListTag: Bytes32 }>,
): Bytes32 {
  const checkedOwner = copyBytes(input.ownerPkHash, 32, "ownerPkHash") as Bytes32;
  const ringSalt = copyBytes(input.ringSalt, 32, "ringSalt") as Bytes32;
  const ringListTag = copyBytes(input.ringListTag, 32, "ringListTag") as Bytes32;
  if (isZero(ringListTag) || !equalBytes(listTagOf(ringSalt), ringListTag))
    throw refused("ringSaltUncommitted");
  return ringListKey(checkedOwner, ringSalt);
}

/** Salts as Ring RPC or an address supplied them, a key exists only under a salt opening its on-chain tag. */
export type OwnerListSalts = Readonly<{
  listSalt?: Bytes32;
  ringSalt?: Bytes32;
}>;

/** The on-chain tags an owner's private list keys open against. */
export type OwnerListTags = Readonly<{
  ownerPkHash: Bytes32;
  /** The tag of the owner's tracking row, absent while it holds none. */
  listTag?: Bytes32;
  /** `RingPolicy.ringListTag`, zero while the policy commits no ring salt. */
  ringListTag: Bytes32;
}>;

export type PrivateListTarget = Readonly<{
  member: Bytes32;
  kind: "owner" | "ring";
}>;

/** Rust `OwnerListSalts::revocation_members`, a clear of fewer keys leaves the entry in force. */
export function privateRevocationKeys(
  salts: OwnerListSalts,
  tags: OwnerListTags,
): readonly PrivateListTarget[] {
  const ringListTag = copyBytes(tags.ringListTag, 32, "ringListTag") as Bytes32;
  const targets: PrivateListTarget[] = [];
  if (tags.listTag !== undefined) {
    if (salts.listSalt === undefined) throw refused("missingListSalt");
    const listSalt = copyBytes(salts.listSalt, 32, "listSalt") as Bytes32;
    if (!equalBytes(listTagOf(listSalt), copyBytes(tags.listTag, 32, "listTag")))
      throw refused("listSaltMismatch");
    targets.push(
      Object.freeze({ member: privateListKey(tags.ownerPkHash, listSalt), kind: "owner" }),
    );
  }
  if (tags.listTag === undefined || !isZero(ringListTag)) {
    if (salts.ringSalt === undefined) throw refused("missingRingSalt");
    targets.push(
      Object.freeze({
        member: committedRingListKey({
          ownerPkHash: tags.ownerPkHash,
          ringSalt: salts.ringSalt,
          ringListTag,
        }),
        kind: "ring",
      }),
    );
  }
  return Object.freeze(targets);
}

function refused(reason: string): RingError {
  return new RingError("RING_ENTRY_INVALID", { details: { reason } });
}

const ZERO_SALT = new Uint8Array(32) as Bytes32;

function lookupKey(ownerPkHash: Bytes32, listSalt: Bytes32): Bytes32 {
  return poseidon([
    domainField(RingDomain.listPosition),
    copyBytes(ownerPkHash, 32, "ownerPkHash"),
    copyBytes(listSalt, 32, "listSalt"),
  ]) as Bytes32;
}
