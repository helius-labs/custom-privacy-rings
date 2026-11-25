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
  if (isZero(checkedSalt))
    throw new RingError("RING_ENTRY_INVALID", { details: { reason: "zeroSalt" } });
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
    throw new RingError("RING_ENTRY_INVALID", { details: { reason: "ringSaltUncommitted" } });
  return ringListKey(checkedOwner, ringSalt);
}

export type OwnerListSalts = Readonly<{
  listSalt?: Bytes32;
  ringSalt?: Bytes32;
}>;

export type PrivateListTarget = Readonly<{
  member: Bytes32;
  kind: "owner" | "ring";
}>;

/** Circuit reads the union of the owner salted key and the ring salted key. */
export function privateRevocationKeys(
  ownerPkHash: Bytes32,
  salts: OwnerListSalts,
): readonly PrivateListTarget[] {
  const checkedOwner = copyBytes(ownerPkHash, 32, "ownerPkHash") as Bytes32;
  const targets: PrivateListTarget[] = [];
  if (salts.listSalt !== undefined) {
    targets.push(
      Object.freeze({ member: privateListKey(checkedOwner, salts.listSalt), kind: "owner" }),
    );
  }
  if (salts.ringSalt !== undefined) {
    const ringSalt = copyBytes(salts.ringSalt, 32, "ringSalt") as Bytes32;
    if (isZero(ringSalt))
      throw new RingError("RING_ENTRY_INVALID", { details: { reason: "zeroSalt" } });
    targets.push(Object.freeze({ member: ringListKey(checkedOwner, ringSalt), kind: "ring" }));
  }
  if (targets.length === 0)
    throw new RingError("RING_ENTRY_INVALID", { details: { reason: "missingListSalt" } });
  return Object.freeze(targets);
}

const ZERO_SALT = new Uint8Array(32) as Bytes32;

function lookupKey(ownerPkHash: Bytes32, listSalt: Bytes32): Bytes32 {
  return poseidon([
    domainField(RingDomain.listPosition),
    copyBytes(ownerPkHash, 32, "ownerPkHash"),
    copyBytes(listSalt, 32, "listSalt"),
  ]) as Bytes32;
}
