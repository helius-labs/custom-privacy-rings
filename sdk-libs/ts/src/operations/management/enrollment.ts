/**
 * A nonzero key enrolls in the ring's tracking registry, Rust `custom_ring_sdk::enrollment`.
 *
 * Ring RPC alone writes an enrollment on chain, batching owner-signed writes into one `EnrollBatch`.
 * A first enrollment is an Enroll write of its record and `RegisterKey` proof, the owner signs
 * the `signedWrite` message over its entry hash, and an owner's state is its leaf under the newest
 * root. A write Ring RPC holds grants nothing until a root holds the leaf. Read before
 * `viewing-rotation.ts`.
 */
import type { Address, Instruction } from "@solana/kit";

import type { ChainReader } from "@heliuslabs/spr/client";
import {
  SYSTEM_PROGRAM,
  Writer,
  addressBytes,
  equalBytes,
  hashBytes,
  instruction,
  meta,
  rightAlign,
  signerAddress,
  ZERO_NULLIFIER_PK,
  type SignerAccount,
} from "@heliuslabs/spr/internal";
import type { Bytes32, Bytes33, Bytes64, RequestContext } from "@heliuslabs/spr/interface";
import { X25519PublicKey, type ShieldedKeypairLike } from "@heliuslabs/spr/keypair";

import { listTagOf } from "../../crypto/keys/list-salt.js";
import { checkedCustomRingProof } from "../../protocol/codec/ring-codecs.js";
import { RingError } from "../../protocol/error.js";
import {
  sealViewingKey,
  type ServedTrackingOpening,
  type TrackingSeal,
} from "../../protocol/indexer/tracking-codec.js";
import { fetchRingPolicy } from "../../protocol/membership/config.js";
import { checkedViewingU } from "../../protocol/membership/member-opening.js";
import { memberOfTag } from "../../protocol/membership/policy.js";
import { verifyTrackingOpening, type TrackingRecord } from "../../protocol/membership/tracking.js";
import { ownerGrantPda, STORAGE_PROGRAM_ID } from "../../protocol/pda/index.js";
import { registerKeyStatementHash } from "../../protocol/proof/statement.js";
import type { SignedEnrollment } from "../../protocol/ring-rpc/enrollment-write.js";
import type { RingProver, TrackingReader } from "../../rpc/ports.js";
import { fetchTrackingRegistry, readTrackingOpenings } from "../../rpc/tracking-resolver.js";
import { readerKeyBytes, type ReaderKey } from "./reader-access.js";
import { sealNullifierKey } from "./register-key.js";

export const OWNER_GRANT_TAG = 8;

/** The fields a first enrollment proves, fixed once its leaf inserts. */
export interface EnrollmentSeal {
  readonly nullifierPk: Bytes32;
  readonly listTag: Bytes32;
  readonly ephU: Bytes32;
  readonly ciphertext: Bytes64;
}

export interface Enroll {
  readonly ringProgramId: Address;
  readonly owner: Address;
  readonly seal: EnrollmentSeal;
  readonly viewingU: Bytes32;
  /** `proveEnrollment`'s `RegisterKey` proof. */
  readonly proof: Uint8Array;
}

/** The owner's leaf under the newest root, its viewing key checked against the leaf. */
export interface EnrollmentState {
  readonly root: Bytes32;
  readonly record: TrackingRecord;
  readonly seal: EnrollmentSeal;
  readonly viewingU: Bytes32;
  /** Rotation counter in the leaf's `viewingHash`. */
  readonly seq: bigint;
}

export type EnrollmentReadClient = Pick<ChainReader, "getAccount"> &
  Pick<TrackingReader, "getTrackingOpenings">;

export async function enrollmentRequest(
  input: Enroll,
  client: EnrollmentReadClient,
  context?: RequestContext,
): Promise<SignedEnrollment> {
  const { ringProgramId, owner, seal } = input;
  // 1. The ring pins its tracking registry, the request names no other.
  const { tracking } = await fetchRingPolicy(client, ringProgramId, context);
  // 2. The zero key seals no secret, and a present owner rotates in place of enrolling.
  checkEnrollment({
    seal,
    stored: await enrollmentState({ trackingRegistry: tracking, owner }, client, context),
  });
  const record = Object.freeze({ ...seal, viewingU: checkedViewingU(input.viewingU) });
  const proof = checkedCustomRingProof(input.proof);
  return Object.freeze({
    ring: ringProgramId,
    registry: tracking,
    write: Object.freeze({ owner, change: Object.freeze({ kind: "enroll", record, proof }) }),
  });
}

/** `undefined` while no root holds the owner's leaf. */
export async function enrollmentState(
  input: Readonly<{ trackingRegistry: Address; owner: Address }>,
  client: EnrollmentReadClient,
  context?: RequestContext,
): Promise<EnrollmentState | undefined> {
  const { trackingRegistry, owner } = input;
  const member = memberOfTag(addressBytes(owner, "owner"));
  const read = await readTrackingOpenings({ client, trackingRegistry, members: [member] }, context);
  const root = read.root.hash;
  const opening = read.openings[0] as ServedTrackingOpening;
  const record = verifyTrackingOpening(opening, root, member);
  if (record === undefined) return undefined;
  if (opening.kind !== "present" || opening.seal === undefined || opening.seal.owner !== owner)
    throw new RingError("RING_TRACKING_INVALID", { details: { reason: "seal" } });
  const { seal } = opening;
  return Object.freeze({
    root,
    record,
    seal: storedSeal(seal, record),
    viewingU: sealViewingKey(seal, record),
    seq: seal.seq,
  });
}

export function checkEnrollment(
  input: Readonly<{
    seal: EnrollmentSeal;
    stored: EnrollmentState | undefined;
  }>,
): void {
  const { seal, stored } = input;
  if (equalBytes(seal.nullifierPk, ZERO_NULLIFIER_PK))
    throw new RingError("RING_ENROLLMENT_ZERO_KEY");
  // An Enroll names a zero prior viewing hash, the apply refuses it once the leaf exists.
  if (stored !== undefined) throw new RingError("RING_ALREADY_ENROLLED");
}

/** Seals the nullifier key to the registry's auditor and proves it. */
export async function proveEnrollment(
  input: Readonly<{
    holder: Pick<ShieldedKeypairLike, "nullifierKey">;
    owner: Address;
    trackingRegistry: Address;
    /** `RingKeys.listSalt` of the ring. */
    listSalt: Bytes32;
    client: EnrollmentReadClient;
    prover: RingProver;
  }>,
  context?: RequestContext,
): Promise<Readonly<{ seal: EnrollmentSeal; proof: Uint8Array }>> {
  const registry = await fetchTrackingRegistry(input.client, input.trackingRegistry, context);
  const auditor = X25519PublicKey.fromBytes(registry.auditorPubkey);
  const member = memberOfTag(addressBytes(input.owner, "owner"));
  const nullifierKey = await input.holder.nullifierKey();
  const secret = nullifierKey.secretBytes();
  const nullifierSecret = rightAlign(secret);
  secret.fill(0);
  const envelope = sealNullifierKey(nullifierKey, input.listSalt, auditor);
  try {
    const seal: EnrollmentSeal = Object.freeze({
      nullifierPk: envelope.nullifierPublicKey,
      listTag: listTagOf(input.listSalt),
      ephU: envelope.sealed.ephemeralPublicKey.u(),
      ciphertext: envelope.sealed.ciphertext,
    });
    const proof = await input.prover.proveRing(
      {
        kind: "registerKey",
        publicInputHash: registerKeyStatementHash({
          ringId: registry.ringId,
          member,
          nullifierPk: seal.nullifierPk,
          listTag: seal.listTag,
          auditorPubkey: registry.auditorPubkey as Bytes33,
          ephU: seal.ephU,
          ciphertext: seal.ciphertext,
        }),
        ringId: registry.ringId,
        member,
        nullifierSecret,
        listSalt: input.listSalt,
        ephSk: envelope.ephemeralSecret,
        auditorPk: auditor.u(),
      },
      context,
    );
    return Object.freeze({ seal, proof: checkedCustomRingProof(proof) });
  } finally {
    nullifierSecret.fill(0);
    envelope.ephemeralSecret.fill(0);
    nullifierKey.destroy();
  }
}

/** The leaf binds the ciphertext by hash, the ephemeral key by nothing. */
function storedSeal(seal: TrackingSeal, record: TrackingRecord): EnrollmentSeal {
  if (!equalBytes(hashBytes(seal.ciphertext), record.ctHash))
    throw new RingError("RING_TRACKING_INVALID", { details: { reason: "ctHash" } });
  return Object.freeze({
    nullifierPk: record.nullifierPk,
    listTag: record.listTag,
    ephU: ephemeralU(seal.ephPk),
    ciphertext: sealCiphertext(seal.ciphertext),
  });
}

function ephemeralU(ephPk: Uint8Array): Bytes32 {
  try {
    return X25519PublicKey.fromBytes(ephPk as Bytes33).u();
  } catch (cause) {
    throw new RingError("RING_KEY_ENVELOPE_INVALID", { cause });
  }
}

function sealCiphertext(ciphertext: Uint8Array): Bytes64 {
  if (ciphertext.length !== 64) throw new RingError("RING_KEY_ENVELOPE_INVALID");
  return ciphertext as Bytes64;
}

export interface WriteOwnerGrant {
  readonly payer: SignerAccount;
  /** Program owners sign through CPI. */
  readonly owner: SignerAccount;
  readonly trackingRegistry: Address;
  readonly reader: ReaderKey;
  readonly active: boolean;
}

export async function ownerGrantInstruction(input: WriteOwnerGrant): Promise<Instruction> {
  const reader = readerKeyBytes(input.reader);
  const [grant] = await ownerGrantPda(input.trackingRegistry, signerAddress(input.owner), reader);
  return instruction(
    new Writer()
      .u8(OWNER_GRANT_TAG, "tag")
      .bytes(reader, reader.length, "reader")
      .u8(input.active ? 1 : 0, "active")
      .finish(),
    [
      meta(input.payer, true, true),
      meta(input.owner, true, false),
      meta(input.trackingRegistry, false, false),
      meta(grant, false, true),
      meta(SYSTEM_PROGRAM, false, false),
    ],
    STORAGE_PROGRAM_ID,
  );
}
