import { ed25519 } from "@noble/curves/ed25519.js";
import { getBase58Decoder, getBase58Encoder, type MessagePartialSigner } from "@solana/kit";

import type { Address, Bytes32, Signature } from "@heliuslabs/spr/interface";
import {
  U64_MAX,
  ZERO_32,
  addressBytes,
  bigIntBytes,
  hashBytes,
  indexerScalars,
  poseidon,
} from "@heliuslabs/spr/internal";

import { RingDomain, domainField } from "../../crypto/derivation/ring-domain.js";
import { ringIdField } from "../../crypto/derivation/ring-stream.js";
import { signMessage } from "../../rpc/ring-rpc.js";
import {
  AUTHORITY_WRITABLE_LISTS,
  MEMBER_WRITABLE_LISTS,
} from "../../operations/management/record-writes.js";
import { RingError } from "../error.js";
import type { RecordWrite } from "../membership/member-registry.js";
import { checkMember, writeChain } from "../membership/member-opening.js";
import { ListId, recordBit, walletGrantMember, type Member } from "../membership/policy.js";
import type {
  RecordUpdate,
  RecordUpdateEntry,
  SubmitRecordUpdateRequest,
} from "./ring-rpc-types.js";

const base58Decoder = getBase58Decoder();
const base58Encoder = getBase58Encoder();
const encoder = new TextEncoder();

const RECORD_UPDATE_DOMAIN = "zolana/ring-rpc-record-update";

/** Rust `ListSet::STORAGE_WRITES`. */
const STORAGE_WRITE_BITS = [...AUTHORITY_WRITABLE_LISTS, ...MEMBER_WRITABLE_LISTS].reduce(
  (bits, id) => bits | recordBit(id),
  0n,
);

/** Rust `AWAITABLE_LISTS`. */
export const AWAITABLE_LISTS: readonly ListId[] = Object.freeze([
  ListId.allow,
  ListId.reader,
  ListId.approval,
]);

export interface UnsignedRecordUpdate {
  readonly registry: Address;
  readonly update: RecordUpdate;
}

/** Rust `RecordUpdateEntry::entry`, the leaf write the apply proof hashes. */
export function recordUpdateWrite(entry: RecordUpdateEntry): RecordWrite {
  switch (entry.kind) {
    case "write": {
      const member = checkMember(indexerScalars.hashBytes(entry.member));
      if (entry.mask === 0n && entry.value === 0n) {
        if (entry.mintPolicy === null) throw invalid("EmptyWrite");
        return { member, mask: 0n, value: 0n, mintPolicy: entry.mintPolicy };
      }
      if (entry.mask === 0n || (entry.value & ~entry.mask) !== 0n)
        throw invalid("ValueOutsideMask");
      if ((entry.mask & ~STORAGE_WRITE_BITS) !== 0n) throw invalid("ListBitNotWritable");
      return {
        member,
        mask: entry.mask,
        value: entry.value,
        ...(entry.mintPolicy === null ? {} : { mintPolicy: entry.mintPolicy }),
      };
    }
    case "walletGrant": {
      const bit = recordBit(ListId.reader);
      return {
        member: walletGrantMember(
          indexerScalars.hashBytes(entry.wallet) as Member,
          indexerScalars.hashBytes(entry.reader) as Member,
        ),
        mask: bit,
        value: entry.access === "granted" ? bit : 0n,
      };
    }
  }
}

/**
 * Mirrored by `RecordUpdateStatement::hash` in `sdk-libs/client/src/rpc/record_update.rs`.
 *   `Poseidon(CRRU, hash_bytes(genesis_hash), hash_bytes(ring), hash_bytes(registry), entries_hash, valid_until_slot[, awaited_lists])`
 */
export function recordUpdateStatementHash(input: UnsignedRecordUpdate): Bytes32 {
  const { genesisHash, ring, entries, validUntilSlot } = input.update;
  const awaitedLists = input.update.awaitedLists ?? 0n;
  if (validUntilSlot < 0n || validUntilSlot > U64_MAX) throw invalid("ValidUntilSlot");
  if (awaitedLists < 0n || awaitedLists > U64_MAX) throw invalid("AwaitedLists");
  return poseidon([
    domainField(RingDomain.recordUpdate),
    hashBytes(indexerScalars.hashBytes(genesisHash)) as Bytes32,
    ringIdField(ring),
    hashBytes(addressBytes(input.registry, "registry")) as Bytes32,
    entries.map(recordUpdateWrite).reduce(writeChain, ZERO_32),
    bigIntBytes(validUntilSlot) as Bytes32,
    // The width binds the wait to the signature, a pending update never verifies as a plain one.
    ...(awaitedLists === 0n ? [] : [bigIntBytes(awaitedLists) as Bytes32]),
  ]);
}

/** Rust `RecordUpdateStatement::message`, the text `signer` signs. */
export function recordUpdateMessage(input: UnsignedRecordUpdate, signer: Address): Uint8Array {
  return messageText(input, signer, recordUpdateStatementHash(input));
}

/** Rust `RecordUpdate::sign`, the writer refuses a signer that does not hold every entry. */
export async function signRecordUpdate(
  input: UnsignedRecordUpdate,
  signer: MessagePartialSigner,
): Promise<SubmitRecordUpdateRequest> {
  const signature = await signMessage(signer, recordUpdateMessage(input, signer.address));
  return Object.freeze({
    registry: input.registry,
    update: input.update,
    signer: signer.address,
    signature: base58Decoder.decode(signature) as Signature,
  });
}

/** Rust `SubmitRecordUpdateRequest::verify`, signature only, the writer checks what the signer may write. */
export function verifyRecordUpdate(request: SubmitRecordUpdateRequest): Bytes32 {
  const hash = recordUpdateStatementHash(request);
  const signature = base58Encoder.encode(request.signature);
  const message = messageText(request, request.signer, hash);
  if (!ed25519.verify(new Uint8Array(signature), message, addressBytes(request.signer, "signer")))
    throw invalid("Signature");
  return hash;
}

function messageText(input: UnsignedRecordUpdate, signer: Address, hash: Bytes32): Uint8Array {
  return encoder.encode(
    [
      RECORD_UPDATE_DOMAIN,
      `cluster: ${base58Decoder.decode(indexerScalars.hashBytes(input.update.genesisHash))}`,
      `ring: ${input.update.ring}`,
      `registry: ${input.registry}`,
      `signer: ${signer}`,
      `update: ${base58Decoder.decode(hash)}`,
      `valid until slot: ${input.update.validUntilSlot}`,
    ].join("\n"),
  );
}

function invalid(reason: string): RingError {
  return new RingError("RING_RECORD_UPDATE_INVALID", { details: { reason } });
}
