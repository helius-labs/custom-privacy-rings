import { bytesKey, equalBytes, isCanonicalField, isZero } from "@heliuslabs/spr/internal";
import type { Address, Bytes16, Bytes32, RequestContext } from "@heliuslabs/spr/interface";
import type { X25519PublicKey, ShieldedAddress, ViewingKey } from "@heliuslabs/spr/keypair";
import type {
  AssetRegistry,
  IndexedShieldedTransaction,
  OutputSlot,
  ShieldedKeys,
  Utxo,
} from "@heliuslabs/spr/transaction";
import { txKeySlotSecret } from "../../crypto/derivation/ring-stream.js";
import { isSmallOrder, x25519FromU } from "../../crypto/keys/x25519.js";
import type { ReplyClaim } from "../../protocol/codec/anonymous-delivery.js";
import { parseAuditorMessage } from "../../crypto/audit/audit.js";
import { openOutputSlot, openSealedDisclosure } from "../../crypto/audit/ring-disclosure.js";
import { RecordKind, type OutputRecord } from "../../protocol/codec/audit-stream.js";
import {
  auditorMessageLen,
  recipientBodyForms,
  type AuditStreamLayout,
} from "../../protocol/codec/ring-disclosure-layout.js";
import { TRANSFER_SHAPES } from "../../protocol/proof/transfer-shapes.js";
import { ringBodyOutput } from "../../protocol/transaction/output.js";
import { openRecipientBody } from "../../crypto/derivation/blinding.js";
import { answered, decoded } from "../batch.js";
import { registeredMint } from "../../operations/workflow/recover.js";
import { claimOf } from "../ring-sync.js";
import type { RingKeys } from "../ring-keys.js";
import type { AnonymousStream, SentPayment, BodyReads } from "./scan-result.js";

/** Rust `RingDeliveryContext`. */
export interface SlotContext {
  readonly ring: Address;
  readonly txViewingPublicKey: X25519PublicKey;
  readonly salt: Bytes16;
  readonly firstNullifier: Bytes32;
}

/** A ring body a held key may open, Rust `Body`. */
export interface Body {
  readonly tx: IndexedShieldedTransaction;
  readonly position: number;
  readonly ciphertext: Uint8Array;
  readonly reply: Uint8Array;
  readonly context: SlotContext;
  readonly found: AnonymousStream;
}

type Rebuilt = Readonly<{
  utxo: Utxo;
  nullifierPublicKey: Bytes32;
  dataHash?: Bytes32;
  ringDataHash?: Bytes32;
}>;

/** A body the reader's own keys rebuilt. */
interface RebuiltNote extends Rebuilt {
  readonly body: Body;
  readonly claim: ReplyClaim | undefined;
  readonly pending: boolean;
}

/** One viewing key's reading of a batch of bodies. */
export class BodyRead {
  readonly notes: RebuiltNote[] = [];
  readonly unknownMints: Address[] = [];
  readonly sent: SentPayment[] = [];
  readonly #reads: BodyReads;
  readonly #owner: ShieldedAddress;

  constructor(reads: BodyReads) {
    this.#reads = reads;
    this.#owner = reads.keys.address();
  }

  /** Only a rebuilt commitment counts, the cipher authenticates nothing. */
  recipientBody(body: Body, secret: Bytes32): void {
    const slot = body.tx.outputSlots[body.position];
    if (slot === undefined) return;
    const rebuilt = rebuild(this.#reads.assets, this.#owner, slot, body.context.ring, {
      secret,
      ciphertext: body.ciphertext,
    });
    if (rebuilt === undefined) return;
    // A mint the registry lacks holds the note back, the scan still moves past it.
    const mint = rebuilt.utxo.asset;
    const pending = registeredMint(this.#reads.assets, mint) === undefined;
    if (pending) this.unknownMints.push(mint);
    this.notes.push({ ...rebuilt, body, claim: claimOf(body.reply, secret), pending });
  }

  /** Rust `Scan::list_sent`, an unverified payment is listed only under the one candidate layout or one naming a held key. */
  sentPayments(
    tx: IndexedShieldedTransaction,
    context: SlotContext,
    txViewingKey: ViewingKey,
  ): void {
    const held = this.#reads.keys.viewingPublicKeys().map((key) => key.u());
    const holds = (key: Bytes32): boolean => held.some((own) => equalBytes(own, key));
    const candidates = [
      ...new Map(
        sentRecipientKeys(tx, txViewingKey).map((keys) => [keys.map(bytesKey).join(), keys]),
      ).values(),
    ];
    const listed = new Set<number>();
    for (const keys of candidates) {
      const bound = candidates.length === 1 || keys.some(holds);
      for (const [position, recipient] of listedRecipients(keys, holds)) {
        if (listed.has(position)) continue;
        const payment = this.#sentPayment({ tx, context, position, recipient, txViewingKey });
        if (payment === undefined || (!bound && payment.check.kind !== "verified")) continue;
        listed.add(position);
        this.sent.push(payment);
      }
    }
  }

  /** `undefined` on an exit, on a dummy and on a body `recipient` does not open. */
  #sentPayment(
    sealed: Readonly<{
      tx: IndexedShieldedTransaction;
      context: SlotContext;
      position: number;
      recipient: X25519PublicKey;
      txViewingKey: ViewingKey;
    }>,
  ): SentPayment | undefined {
    const { tx, context, position, recipient } = sealed;
    const slot = tx.outputSlots[position];
    const parts = slot === undefined ? undefined : ringBodyOutput(slot.payload);
    if (slot === undefined || parts === undefined) return undefined;
    const secret = txKeySlotSecret({
      txViewingKey: sealed.txViewingKey,
      recipientViewingKey: recipient,
      salt: context.salt,
      outputIndex: position,
      firstNullifier: context.firstNullifier,
    });
    let record: OutputRecord | undefined;
    let verified: ShieldedAddress | undefined;
    try {
      // The amount sits in the record's head, the same bytes under every form.
      for (const form of recipientBodyForms(parts.body.length)) {
        record = decoded(() => openOutputSlot(secret, parts.body, form))?.record;
        if (record !== undefined) break;
      }
      // A dummy's key is in the stream too, its proven record names no note.
      if (record?.kind !== RecordKind.ring) return undefined;
      const body = { secret, ciphertext: parts.body };
      verified = this.#reads.recipients.find(
        (address) =>
          address.viewingPublicKey.equals(recipient) &&
          rebuild(this.#reads.assets, address, slot, context.ring, body) !== undefined,
      );
    } finally {
      secret.fill(0);
    }
    return Object.freeze({
      txSignature: tx.txSignature,
      utxoHash: slot.outputContext.hash,
      position,
      recipientViewingPublicKey: recipient,
      ownerPublicKey: verified?.signingPublicKey,
      amount: record.amount,
      check: Object.freeze(
        verified === undefined
          ? { kind: "unverified" as const }
          : { kind: "verified" as const, mint: record.mint },
      ),
    });
  }
}

/** Each body's slot secret, one agreement per transaction inside the holder. */
export async function bodySecrets(
  keys: RingKeys,
  viewingPublicKey: X25519PublicKey,
  bodies: readonly Body[],
  context: RequestContext | undefined,
): Promise<readonly (readonly [Body, Bytes32])[]> {
  if (bodies.length === 0) return [];
  return answered(
    bodies,
    await keys.ringDeliverySecrets(
      bodies.map((body) => ({
        viewingPublicKey,
        txViewingPublicKey: body.context.txViewingPublicKey,
        salt: body.context.salt,
        outputIndex: body.position,
        firstNullifier: body.context.firstNullifier,
      })),
      context,
    ),
  );
}

/** Wipes every secret it reads. */
export async function readBodies(
  reads: BodyReads,
  opened: readonly (readonly [Body, Bytes32])[],
  visit: (open: () => BodyRead) => Promise<void>,
): Promise<void> {
  try {
    for (const [body, secret] of opened)
      await visit(() => {
        const read = new BodyRead(reads);
        read.recipientBody(body, secret);
        return read;
      });
  } finally {
    for (const [, secret] of opened) secret.fill(0);
  }
}

/** The transaction key of each row the entry's key sent, the caller destroys each. */
export async function ownTransactionKeys(
  keys: ShieldedKeys,
  viewingPublicKey: X25519PublicKey,
  rows: readonly IndexedShieldedTransaction[],
  context: RequestContext | undefined,
): Promise<readonly (readonly [IndexedShieldedTransaction, SlotContext, ViewingKey])[]> {
  const sealed = rows.flatMap((tx) => {
    const slot = slotContext(tx);
    return slot === undefined ? [] : [[tx, slot] as const];
  });
  if (sealed.length === 0) return [];
  const derived = answered(
    sealed,
    await keys.transactionKeys(
      sealed.map(([, slot]) => ({ viewingPublicKey, firstNullifier: slot.firstNullifier })),
      context,
    ),
  );
  return derived.flatMap(([[tx, slot], txViewingKey]) => {
    if (txViewingKey.publicKey().equals(slot.txViewingPublicKey))
      return [[tx, slot, txViewingKey] as const];
    txViewingKey.destroy();
    return [];
  });
}

/** Tries the registered key then the zero key, a third key misses. */
function rebuild(
  assets: AssetRegistry,
  owner: ShieldedAddress,
  slot: OutputSlot,
  ring: Address,
  sealed: Readonly<{ secret: Bytes32; ciphertext: Uint8Array }>,
): Rebuilt | undefined {
  const { hash, treeId } = slot.outputContext;
  if (treeId === undefined) return undefined;
  return decoded(() =>
    openRecipientBody(
      { owner, ring, treeId, commitment: hash, assets },
      sealed.secret,
      sealed.ciphertext,
    ),
  );
}

/**
 * `V_k` of every slot from the auditor stream under the sender's own transaction key, Rust
 * `AuditEnvelope::open_sealed`, the auditor key read from the last message's view tag. The frame
 * names no key class, and an entry key's envelope moves the key counters, so each class the
 * catalog keys at `n` gives one candidate.
 */
function sentRecipientKeys(
  tx: IndexedShieldedTransaction,
  txViewingKey: ViewingKey,
): readonly (readonly Bytes32[])[] {
  const { salt } = tx;
  const message = tx.messages.at(-1);
  if (message === undefined || salt === undefined) return [];
  const auditor = decoded(() => x25519FromU(message.viewTag));
  if (auditor === undefined || isSmallOrder(auditor)) return [];
  const exits = tx.outputSlots.filter((slot) => ringBodyOutput(slot.payload) === undefined).length;
  const txViewingSecret = txViewingKey.secretBytes();
  try {
    return senderAuditLayouts(tx.nullifiers.length, tx.outputSlots.length, exits)
      .filter((layout) => auditorMessageLen(layout, exits) === message.data.length)
      .flatMap((layout) => {
        const opened = decoded(() =>
          openSealedDisclosure(txViewingSecret, auditor, {
            layout,
            salt,
            message: parseAuditorMessage(message.data, layout, exits),
          }),
        );
        if (opened === undefined) return [];
        opened.envelope.txViewingSecret.fill(0);
        return [opened.recipientKeys];
      });
  } finally {
    txViewingSecret.fill(0);
  }
}

/**
 * One plain layout, the plain envelope fixes every counter, then one per entry class holding `n`,
 * each unvouched and vouched, and under an exit each with the data bit its record length reads.
 */
function senderAuditLayouts(
  inputs: number,
  outputs: number,
  exits: number,
): readonly AuditStreamLayout[] {
  const entryClasses = new Set(
    TRANSFER_SHAPES.filter((shape) => shape.entry && shape.inputs >= inputs).map(
      (shape) => shape.inputs,
    ),
  );
  const keys = [
    { class: inputs, entry: false },
    ...[...entryClasses].map((keyClass) => ({ class: keyClass, entry: true })),
  ];
  const forms = (exits === 0 ? [false] : [false, true]).map((data) => ({ data, ringData: false }));
  return keys.flatMap((key) =>
    [false, true].flatMap((vouched) =>
      forms.map((records) => ({ key, inputs, outputs, capacity: outputs, vouched, records })),
    ),
  );
}

/** Key `k` names slot `k`, zero keys, the wallet's own change keys and keys no ECDH accepts left out. */
function listedRecipients(
  keys: readonly Bytes32[],
  holds: (key: Bytes32) => boolean,
): readonly (readonly [number, X25519PublicKey])[] {
  return keys.flatMap((key, position) => {
    if (isZero(key) || holds(key)) return [];
    const recipient = decoded(() => x25519FromU(key));
    return recipient === undefined || isSmallOrder(recipient)
      ? []
      : [[position, recipient] as const];
  });
}

/** Every ring body of `tx`, none selected by its tag, Rust `Body::every`. */
export function ringBodies(tx: IndexedShieldedTransaction): readonly Body[] {
  const context = slotContext(tx);
  if (context === undefined) return [];
  return tx.outputSlots.flatMap((slot, position) => {
    const parts = ringBodyOutput(slot.payload);
    return parts === undefined
      ? []
      : [{ tx, position, ciphertext: parts.body, reply: parts.reply, context, found: FULL_SCAN }];
  });
}

const FULL_SCAN: AnonymousStream = Object.freeze({ kind: "fullScan" });

/** `undefined` without a ring, a contributory key, a salt or a canonical first nullifier. */
function slotContext(tx: IndexedShieldedTransaction): SlotContext | undefined {
  const { ringProgramId: ring, txViewingPublicKey, salt } = tx;
  const firstNullifier = tx.nullifiers[0];
  if (
    ring === undefined ||
    txViewingPublicKey === undefined ||
    salt === undefined ||
    firstNullifier === undefined ||
    isSmallOrder(txViewingPublicKey) ||
    !isCanonicalField(firstNullifier)
  )
    return undefined;
  return { ring, txViewingPublicKey, salt, firstNullifier };
}
