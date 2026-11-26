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
import { anonymousSlotTag, txKeySlotSecret } from "../../crypto/derivation/ring-stream.js";
import { isSmallOrder, x25519FromU } from "../../crypto/keys/x25519.js";
import {
  openSenderBundle,
  SENDER_BUNDLE_LEN,
  type ReplyClaim,
} from "../../protocol/codec/anonymous-delivery.js";
import { parseAuditorMessage } from "../../crypto/audit/audit.js";
import { openOutputSlot, openSealedDisclosure } from "../../crypto/audit/ring-disclosure.js";
import type { OutputRecord } from "../../protocol/codec/audit-stream.js";
import {
  auditorMessageLen,
  recipientBodyForms,
  type AuditStreamLayout,
} from "../../protocol/codec/ring-disclosure-layout.js";
import { TRANSFER_SHAPES } from "../../protocol/proof/transfer-shapes.js";
import { ringBodyOutput, senderBundleOutputBody } from "../../protocol/transaction/output.js";
import { openRecipientBody } from "../../crypto/derivation/blinding.js";
import { answered, decoded } from "../batch.js";
import { registeredMint } from "../../operations/workflow/recover.js";
import { claimOf } from "../ring-sync.js";
import type { RingKeys, RingTagFrame } from "../ring-keys.js";
import { RingError } from "../../protocol/error.js";
import type { AnonymousStream, SentPayment, BodyReads } from "./scan-result.js";

/** Rust `RingDeliveryContext`. */
interface SlotContext {
  readonly ring: Address;
  readonly txViewingPublicKey: X25519PublicKey;
  readonly salt: Bytes16;
  readonly firstNullifier: Bytes32;
}

/** A recipient body at an output, a sender bundle in a message. */
type Site =
  | Readonly<{ kind: "output"; position: number }>
  | Readonly<{ kind: "message"; message: number }>;

/** A ring body or a sender bundle a held key may open. */
export interface Body {
  readonly tx: IndexedShieldedTransaction;
  readonly site: Site;
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
  readonly position: number;
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

  recipientBody(body: Body, position: number, secret: Bytes32): void {
    this.#note(body, position, secret, body.ciphertext, claimOf(body.reply, secret));
  }

  /** Rust `SenderBundle` and `Scan::open_bundle`, change candidates are the zero-tag bodies alone. */
  async senderBundle(
    body: Body,
    secret: Bytes32,
    txViewingKey: ViewingKey | undefined,
    changeSecrets: (positions: readonly number[]) => Promise<readonly Bytes32[]>,
  ): Promise<void> {
    const bundle = decoded(() => openSenderBundle(secret, body.ciphertext));
    if (bundle === undefined) return;
    bundle.outputSeed.fill(0);
    const own = this.#owner.viewingPublicKey.u();
    const change = body.tx.outputSlots.flatMap((slot, position) =>
      isZero(slot.viewTag) && ringBodyOutput(slot.payload) !== undefined ? [position] : [],
    );
    const secrets = change.length === 0 ? [] : await changeSecrets(change);
    try {
      change.forEach((position, index) => {
        const slot = body.tx.outputSlots[position];
        const parts = slot === undefined ? undefined : ringBodyOutput(slot.payload);
        const changeSecret = secrets[index];
        if (parts === undefined || changeSecret === undefined) return;
        this.#note(body, position, changeSecret, parts.body, undefined);
      });
    } finally {
      secrets.forEach((changeSecret) => changeSecret.fill(0));
    }
    if (txViewingKey === undefined) return;
    const sent = new Set<number>();
    for (const keys of sentRecipientKeys(body.tx, txViewingKey))
      for (const [position, recipient] of listedRecipients(keys, own)) {
        if (sent.has(position)) continue;
        const payment = this.#sentPayment(body, position, recipient, txViewingKey);
        if (payment === undefined) continue;
        sent.add(position);
        this.sent.push(payment);
      }
  }

  /** Only a rebuilt commitment counts, the cipher authenticates nothing. */
  #note(
    body: Body,
    position: number,
    secret: Bytes32,
    ciphertext: Uint8Array,
    claim: ReplyClaim | undefined,
  ): void {
    const slot = body.tx.outputSlots[position];
    if (slot === undefined) return;
    const rebuilt = rebuild(this.#reads.assets, this.#owner, slot, body.context.ring, {
      secret,
      ciphertext,
    });
    if (rebuilt !== undefined) this.#keep(body, position, rebuilt, claim);
  }

  /** A mint the registry lacks holds the note back, the scan still moves past it. */
  #keep(body: Body, position: number, rebuilt: Rebuilt, claim: ReplyClaim | undefined): void {
    const slot = body.tx.outputSlots[position];
    if (slot === undefined) return;
    const mint = rebuilt.utxo.asset;
    const pending = registeredMint(this.#reads.assets, mint) === undefined;
    if (pending) this.unknownMints.push(mint);
    this.notes.push({ ...rebuilt, body, position, claim, pending });
  }

  /** `undefined` unless `recipient` derives the slot's published tag and the body opens under it. */
  #sentPayment(
    body: Body,
    position: number,
    recipient: X25519PublicKey,
    txViewingKey: ViewingKey,
  ): SentPayment | undefined {
    const slot = body.tx.outputSlots[position];
    const parts = slot === undefined ? undefined : ringBodyOutput(slot.payload);
    if (slot === undefined || parts === undefined) return undefined;
    const secret = txKeySlotSecret({
      txViewingKey,
      recipientViewingKey: recipient,
      salt: body.context.salt,
      outputIndex: position,
      firstNullifier: body.context.firstNullifier,
    });
    let record: OutputRecord | undefined;
    let verified: ShieldedAddress | undefined;
    try {
      // The tag binds `V_k` to its slot, a key read under another layout derives another tag.
      if (!equalBytes(anonymousSlotTag(secret), slot.viewTag)) return undefined;
      const sealed = { secret, ciphertext: parts.body };
      // The amount sits in the record's head, the same bytes under every form.
      for (const form of recipientBodyForms(parts.body.length)) {
        record = decoded(() => openOutputSlot(secret, parts.body, form))?.record;
        if (record !== undefined) break;
      }
      if (record === undefined) return undefined;
      verified = this.#reads.recipients.find(
        (address) =>
          address.viewingPublicKey.equals(recipient) &&
          rebuild(this.#reads.assets, address, slot, body.context.ring, sealed) !== undefined,
      );
    } finally {
      secret.fill(0);
    }
    return Object.freeze({
      txSignature: body.tx.txSignature,
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
        outputIndex: outputIndex(body),
        firstNullifier: body.context.firstNullifier,
      })),
      context,
    ),
  );
}

/** Wipes every secret it reads. */
export async function readBodies(
  reads: BodyReads,
  viewingPublicKey: X25519PublicKey,
  opened: readonly (readonly [Body, Bytes32])[],
  context: RequestContext | undefined,
  visit: (open: () => Promise<BodyRead>) => Promise<void>,
): Promise<void> {
  let own: ReadonlyMap<string, ViewingKey> = new Map();
  try {
    own = await ownTransactions(
      reads.keys,
      viewingPublicKey,
      opened.map(([body]) => body),
      context,
    );
    for (const [body, secret] of opened)
      await visit(async () => {
        const read = new BodyRead(reads);
        if (body.site.kind === "output") read.recipientBody(body, body.site.position, secret);
        else
          await read.senderBundle(
            body,
            secret,
            own.get(bytesKey(body.context.firstNullifier)),
            async (positions) =>
              answered(
                positions,
                await reads.keys.ringDeliverySecrets(
                  positions.map((outputIndex) => ({
                    viewingPublicKey,
                    txViewingPublicKey: body.context.txViewingPublicKey,
                    salt: body.context.salt,
                    outputIndex,
                    firstNullifier: body.context.firstNullifier,
                  })),
                  context,
                ),
              ).map(([, changeSecret]) => changeSecret),
          );
        return read;
      });
  } finally {
    for (const [, secret] of opened) secret.fill(0);
    for (const txViewingKey of own.values()) txViewingKey.destroy();
  }
}

/** The transaction key of each transaction the entry's key sent, keyed by its first nullifier. */
export async function ownTransactions(
  keys: ShieldedKeys,
  viewingPublicKey: X25519PublicKey,
  bodies: readonly Body[],
  context: RequestContext | undefined,
): Promise<ReadonlyMap<string, ViewingKey>> {
  const contexts = new Map<string, SlotContext>();
  for (const body of bodies)
    if (body.site.kind === "message")
      contexts.set(bytesKey(body.context.firstNullifier), body.context);
  if (contexts.size === 0) return new Map();
  const held = [...contexts.values()];
  const derived = answered(
    held,
    await keys.transactionKeys(
      held.map((slot) => ({
        viewingPublicKey,
        firstNullifier: slot.firstNullifier,
      })),
      context,
    ),
  );
  const own = new Map<string, ViewingKey>();
  derived.forEach(([slot, txViewingKey]) => {
    // Another viewing key sent the transaction.
    if (!txViewingKey.publicKey().equals(slot.txViewingPublicKey)) {
      txViewingKey.destroy();
      return;
    }
    own.set(bytesKey(slot.firstNullifier), txViewingKey);
  });
  return own;
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
 * catalog keys at `n` gives one candidate and the slot tag decides.
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

/** Key `k` names slot `k`, zero keys, the sender's own change keys and keys no ECDH accepts left out. */
function listedRecipients(
  keys: readonly Bytes32[],
  own: Bytes32,
): readonly (readonly [number, X25519PublicKey])[] {
  return keys.flatMap((key, position) => {
    if (isZero(key) || equalBytes(key, own)) return [];
    const recipient = decoded(() => x25519FromU(key));
    return recipient === undefined || isSmallOrder(recipient)
      ? []
      : [[position, recipient] as const];
  });
}

export function sitesOf(tx: IndexedShieldedTransaction): readonly Site[] {
  return [
    ...tx.outputSlots.map((_, position): Site => ({ kind: "output", position })),
    ...tx.messages.map((_, message): Site => ({ kind: "message", message })),
  ];
}

/** `undefined` for a site that holds no well formed body or bundle. */
export function bodyAt(
  tx: IndexedShieldedTransaction,
  site: Site,
  found: AnonymousStream,
): Body | undefined {
  const context = slotContext(tx);
  if (context === undefined) return undefined;
  if (site.kind === "output") {
    const slot = tx.outputSlots[site.position];
    const parts = slot === undefined ? undefined : ringBodyOutput(slot.payload);
    return parts === undefined
      ? undefined
      : {
          tx,
          site,
          ciphertext: parts.body,
          reply: parts.reply,
          context,
          found,
        };
  }
  const message = tx.messages[site.message];
  const bundle = message === undefined ? undefined : senderBundleOutputBody(message.data);
  // Compact slots never reach the event, a bundle of any other `m` opens under another secret.
  return bundle?.length === SENDER_BUNDLE_LEN
    ? { tx, site, ciphertext: bundle, reply: new Uint8Array(), context, found }
    : undefined;
}

/** A body at its position, a bundle at `m`, past every body's index. */
function outputIndex(body: Body): number {
  return body.site.kind === "output" ? body.site.position : body.tx.outputSlots.length;
}

export const FULL_SCAN: AnonymousStream = Object.freeze({ kind: "fullScan" });
export const TAG_HINT: AnonymousStream = Object.freeze({ kind: "tagHint" });

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

/** A slot answers under the recipient domain, a bundle under the sender's at index `m`. */
export function bodyProbe(body: Body): RingTagFrame["probes"][number] {
  const { site, tx } = body;
  const published =
    site.kind === "output" ? tx.outputSlots[site.position] : tx.messages[site.message];
  if (published === undefined)
    throw new RingError("RING_VIEWING_HISTORY_INVALID", { details: { reason: "no such site" } });
  return {
    kind: site.kind === "output" ? "recipient" : "senderBundle",
    outputIndex: outputIndex(body),
    tag: published.viewTag,
  };
}
