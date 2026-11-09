/**
 * A nonzero key enrolls in the ring's tracking registry, Rust `custom_ring_sdk::enrollment`.
 *
 * `Enroll` queues an entry and `ApplyEnrollments` moves the tree, so an owner's state is the leaf
 * under the newest root or, before an apply reaches it, its proven queued entry. A queued entry
 * grants nothing. Read before `viewing-rotation.ts`.
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
import { MAX_RING_MEMBER_BATCH } from "../../protocol/indexer/indexer-limits.js";
import {
  sealViewingKey,
  type QueuedEnrollment,
  type TrackingSeal,
} from "../../protocol/indexer/tracking-codec.js";
import { QUAD_CAPACITY } from "../../protocol/membership/member-registry.js";
import { checkedViewingU } from "../../protocol/membership/member-opening.js";
import { memberOfTag } from "../../protocol/membership/policy.js";
import {
  checkEnrollmentQueueOpen,
  decodeEnrollmentQueue,
  verifyTrackingOpening,
  type EnrollmentQueue,
  type TrackingRecord,
} from "../../protocol/membership/tracking.js";
import { enrollmentQueuePda, ownerGrantPda, STORAGE_PROGRAM_ID } from "../../protocol/pda/index.js";
import { applyKeyWidth } from "../../protocol/proof/proving-keys.js";
import { registerKeyStatementHash } from "../../protocol/proof/statement.js";
import { enrollAccounts } from "../../protocol/ring-rpc/relay-enroll-inputs.js";
import type { RingProver, TrackingReader } from "../../rpc/ports.js";
import { currentTrackingRoot, fetchTrackingRegistry } from "../../rpc/tracking-resolver.js";
import { readerKeyBytes, type ReaderKey } from "./reader-access.js";
import { sealNullifierKey } from "./register-key.js";

export const ENROLL_TAG = 6;
export const APPLY_ENROLLMENTS_TAG = 7;
export const OWNER_GRANT_TAG = 8;

/** The fields a first enrollment proves and a rotation repeats. */
export interface EnrollmentSeal {
  readonly nullifierPk: Bytes32;
  readonly listTag: Bytes32;
  readonly ephU: Bytes32;
  readonly ciphertext: Bytes64;
}

/** Moves no lamports, so it names no payer. */
export interface Enroll {
  readonly owner: SignerAccount;
  readonly trackingRegistry: Address;
  readonly seal: EnrollmentSeal;
  readonly viewingU: Bytes32;
  /** The `RegisterKey` proof on a first enrollment, none on a rotation. */
  readonly proof?: Uint8Array;
}

/** An applied viewing key is checked against its leaf, a queued state is the owner's first proven entry. */
export type EnrollmentState =
  | Readonly<{
      kind: "applied";
      root: Bytes32;
      record: TrackingRecord;
      seal: EnrollmentSeal;
      viewingU: Bytes32;
    }>
  | Readonly<{ kind: "queued"; sequence: bigint; seal: EnrollmentSeal; viewingU: Bytes32 }>;

export type EnrollmentReadClient = Pick<ChainReader, "getAccount"> &
  Pick<TrackingReader, "getTrackingOpenings" | "getEnrollmentQueue">;

/** An entry of a present owner moves only its viewing key, so a seal off the stored one refuses. */
export async function enrollInstruction(
  input: Enroll,
  client: EnrollmentReadClient,
  context?: RequestContext,
): Promise<Instruction> {
  const owner = signerAddress(input.owner);
  const { seal, trackingRegistry } = input;
  const [stored, queue] = await Promise.all([
    enrollmentState({ trackingRegistry, owner }, client, context),
    readEnrollmentQueue(client, trackingRegistry, context),
  ]);
  checkEnrollment({ seal, stored });
  checkEnrollmentQueueOpen(queue);
  const writer = new Writer()
    .u8(ENROLL_TAG, "tag")
    .bytes(seal.nullifierPk, 32, "nullifierPk")
    .bytes(seal.listTag, 32, "listTag")
    .bytes(checkedViewingU(input.viewingU), 32, "viewingU")
    .bytes(seal.ephU, 32, "ephU")
    .bytes(seal.ciphertext, 64, "ciphertext");
  if (input.proof !== undefined)
    writer.bytes(checkedCustomRingProof(input.proof), input.proof.length, "proof");
  return instruction(writer.finish(), await enrollAccounts(input), STORAGE_PROGRAM_ID);
}

/** `undefined` before the owner's first proven `Enroll`, a queued rotation alone changes nothing. */
export async function enrollmentState(
  input: Readonly<{ trackingRegistry: Address; owner: Address }>,
  client: EnrollmentReadClient,
  context?: RequestContext,
): Promise<EnrollmentState | undefined> {
  const { trackingRegistry, owner } = input;
  // 1. A leaf under the newest root fixes the nullifier key, a later entry of the owner moves
  // only its viewing key.
  const root = currentTrackingRoot(
    await fetchTrackingRegistry(client, trackingRegistry, context),
  ).hash;
  const member = memberOfTag(addressBytes(owner, "owner"));
  const { openings } = await client.getTrackingOpenings(
    { registry: trackingRegistry, members: [member], expectedRoot: root },
    context,
  );
  const [opening] = openings;
  if (opening === undefined || openings.length !== 1)
    throw new RingError("RING_TRACKING_INVALID", { details: { reason: "openings" } });
  const record = verifyTrackingOpening(opening, root, member);
  if (record !== undefined) {
    if (opening.kind !== "present" || opening.seal === undefined || opening.seal.owner !== owner)
      throw new RingError("RING_TRACKING_INVALID", { details: { reason: "seal" } });
    const { seal } = opening;
    return Object.freeze({
      kind: "applied",
      root,
      record,
      seal: storedSeal(seal, record),
      viewingU: sealViewingKey(seal, record),
    });
  }
  // 2. Absent under the root, the queue applies first in first out and the first proven entry
  // inserts the leaf.
  const queued = await firstProvenEntry({ trackingRegistry, owner }, client, context);
  return queued === undefined
    ? undefined
    : Object.freeze({
        kind: "queued",
        sequence: queued.sequence,
        seal: entrySeal(queued),
        viewingU: queued.viewingU,
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
  if (stored === undefined) return;
  if (
    !equalBytes(stored.seal.nullifierPk, seal.nullifierPk) ||
    !equalBytes(stored.seal.listTag, seal.listTag) ||
    !equalBytes(stored.seal.ephU, seal.ephU) ||
    !equalBytes(stored.seal.ciphertext, seal.ciphertext)
  )
    throw new RingError("RING_ENROLLMENT_KEY_MISMATCH");
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

export interface ApplyEnrollments {
  readonly trackingRegistry: Address;
  /** The revision the proof was made against. */
  readonly revision: bigint;
  readonly proof: Uint8Array;
  readonly newRoot: Bytes32;
  readonly newNextIndex: bigint;
  /** The queued entries from `head` the proof applies. */
  readonly count: number;
}

/** Permissionless, the queue fixes the entries and their order. */
export async function applyEnrollmentsInstruction(input: ApplyEnrollments): Promise<Instruction> {
  if (input.count === 0) throw new RingError("RING_ENROLLMENT_NONE_NAMED");
  applyKeyWidth(input.count);
  const [queue] = await enrollmentQueuePda(input.trackingRegistry);
  const { proof } = input;
  return instruction(
    new Writer()
      .u8(APPLY_ENROLLMENTS_TAG, "tag")
      .u64(input.revision, "revision")
      .bytes(checkedCustomRingProof(proof), proof.length, "proof")
      .bytes(input.newRoot, 32, "newRoot")
      .u64(checkedNextIndex(input.newNextIndex), "newNextIndex")
      .u8(input.count, "count")
      .finish(),
    [meta(input.trackingRegistry, false, true), meta(queue, false, true)],
    STORAGE_PROGRAM_ID,
  );
}

/** The queue account at the read commitment, one holding another registry refuses. */
export async function readEnrollmentQueue(
  client: Pick<ChainReader, "getAccount">,
  trackingRegistry: Address,
  context?: RequestContext,
): Promise<EnrollmentQueue> {
  const [address] = await enrollmentQueuePda(trackingRegistry);
  const account = await client.getAccount(address, context);
  if (account === undefined || account.owner !== STORAGE_PROGRAM_ID)
    throw new RingError("RING_TRACKING_INVALID", { details: { reason: "queue" } });
  const queue = decodeEnrollmentQueue(account.data);
  if (queue.registry !== trackingRegistry)
    throw new RingError("RING_TRACKING_INVALID", { details: { reason: "queue registry" } });
  return queue;
}

/** Storage refuses a next index past the tree's capacity. */
function checkedNextIndex(nextIndex: bigint): bigint {
  if (nextIndex < 1n || nextIndex > QUAD_CAPACITY)
    throw new RingError("RING_TRACKING_INVALID", {
      details: { nextIndex: nextIndex.toString() },
    });
  return nextIndex;
}

/** The owner's oldest proven entry the indexer still queues, paged in sequence order. */
async function firstProvenEntry(
  input: Readonly<{ trackingRegistry: Address; owner: Address }>,
  client: Pick<TrackingReader, "getEnrollmentQueue">,
  context: RequestContext | undefined,
): Promise<QueuedEnrollment | undefined> {
  let after: bigint | undefined;
  for (;;) {
    const page = await client.getEnrollmentQueue(
      {
        registry: input.trackingRegistry,
        ...(after === undefined ? {} : { after }),
        limit: MAX_RING_MEMBER_BATCH,
      },
      context,
    );
    const found = page.entries.find((entry) => entry.proven && entry.owner === input.owner);
    if (found !== undefined) return found;
    const last = page.entries.at(-1);
    if (last === undefined || last.sequence + 1n >= page.tail) return undefined;
    after = last.sequence;
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

function entrySeal(entry: QueuedEnrollment): EnrollmentSeal {
  return Object.freeze({
    nullifierPk: entry.nullifierPk,
    listTag: entry.listTag,
    ephU: ephemeralU(entry.ephPk),
    ciphertext: sealCiphertext(entry.ciphertext),
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
