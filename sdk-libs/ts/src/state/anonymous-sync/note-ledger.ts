import { getBase58Encoder, type ReadonlyUint8Array } from "@solana/kit";
import { bytesKey, equalBytes, treeAddress, ZERO_NULLIFIER_PK } from "@heliuslabs/spr/internal";
import type { Address, Bytes32, RequestContext } from "@heliuslabs/spr/interface";
import type { X25519PublicKey, ShieldedAddress } from "@heliuslabs/spr/keypair";
import type {
  AssetRegistry,
  IndexedShieldedTransaction,
  OutputSlot,
  ShieldedKeys,
  Utxo,
  WalletUtxo,
} from "@heliuslabs/spr/transaction";
import { answered, unseen } from "../batch.js";
import { zeroKeyNullifier } from "../exposed-notes.js";
import { registeredMint } from "../../operations/workflow/recover.js";
import { openRingMerges } from "../ring-sync.js";
import type {
  AnonymousRingUtxo,
  SenderClaim,
  SentPayment,
  AnonymousStream,
  AnonymousRecovery,
} from "./scan-result.js";
import type { BodyRead } from "./body-read.js";

/** A note whose rebuilt hash matched, before its nullifier. */
interface Found {
  readonly utxo: Utxo;
  readonly dataHash?: Bytes32;
  readonly ringDataHash?: Bytes32;
  readonly outputContext: WalletUtxo["outputContext"];
  readonly nullifierPublicKey: Bytes32;
  readonly order: Publication;
  /** Under a mint the registry lacks. */
  readonly pending?: boolean;
}

/** Where a note was published, the order a scan returns its notes in. */
interface Publication {
  readonly slot: bigint;
  readonly signature: ReadonlyUint8Array;
  readonly position: number;
}

interface Decrypted {
  readonly utxo: WalletUtxo;
  readonly order: Publication;
}

export interface MergeReads {
  readonly keys: ShieldedKeys;
  readonly owner: ShieldedAddress;
  readonly assets: AssetRegistry;
  readonly ledger: NoteLedger;
  /** Output hash keys of the held notes. */
  readonly held: ReadonlySet<string>;
  readonly context: RequestContext | undefined;
}

/** The merges spending the found and held ring notes, round after round. */
export async function openMerges(
  input: MergeReads,
  spendsOf: (frontier: readonly Bytes32[]) => Promise<readonly IndexedShieldedTransaction[]>,
  heldRing: readonly Bytes32[],
): Promise<void> {
  const queried = new Set<string>();
  let seed = heldRing;
  for (;;) {
    await input.ledger.assignNullifiers(input.context);
    const frontier = [...input.ledger.takeNullifiers(), ...seed].filter((nullifier) =>
      unseen(queried, bytesKey(nullifier)),
    );
    seed = [];
    if (frontier.length === 0) return;
    const spends = await spendsOf(frontier);
    await keepRingMerges(input, spends);
    await input.ledger.assignNullifiers(input.context);
    input.ledger.noteSpends(spends);
  }
}

/** Keeps each merge output of `rows` the owner's keys open and no held or found note holds. */
export async function keepRingMerges(
  input: MergeReads,
  rows: readonly IndexedShieldedTransaction[],
): Promise<void> {
  const { ledger, owner, context } = input;
  const { merges, utxos } = await openRingMerges(
    { keys: input.keys, owner, assets: input.assets, spends: rows },
    context,
  );
  const opened = new Map(utxos.map((utxo) => [bytesKey(utxo.outputContext.hash), utxo]));
  for (const merge of merges) {
    const firstNullifier = merge.nullifiers[0];
    if (firstNullifier === undefined) continue;
    for (const [position, slot] of merge.outputSlots.entries()) {
      const hash = slot.outputContext.hash;
      const key = bytesKey(hash);
      const utxo = opened.get(key);
      if (utxo === undefined || input.held.has(key) || ledger.isFound(hash)) continue;
      const pending = registeredMint(input.assets, utxo.utxo.asset) === undefined;
      if (pending) ledger.unknownMint(utxo.utxo.asset);
      ledger.keep(
        {
          utxo: utxo.utxo,
          pending,
          outputContext: utxo.outputContext,
          nullifierPublicKey: owner.nullifierPublicKey,
          order: publication(merge, position),
        },
        owner.viewingPublicKey,
        Object.freeze({ kind: "merge", firstNullifier }),
      );
    }
  }
}

/** @internal The notes one sync or recovery keeps, each once, under the stream that found it. */
export class NoteLedger {
  readonly #keys: ShieldedKeys;
  /** Only a spend of these or of a found note leaves the ledger, a ring's other spends are not the wallet's. */
  readonly #held: readonly WalletUtxo[];
  #found: Found[] = [];
  readonly #decrypted: Decrypted[] = [];
  readonly #pending: Decrypted[] = [];
  /** Output hash keys of every kept note, one lookup per candidate. */
  readonly #known = new Set<string>();
  readonly #streams: AnonymousRingUtxo[] = [];
  /** Kept apart from `#streams`, a recovery opens merges page by page yet lists them after every body. */
  readonly #mergeStreams: AnonymousRingUtxo[] = [];
  readonly #spent = new Map<string, Bytes32>();
  readonly #ownedNullifiers = new Set<string>();
  #newNullifiers: Bytes32[] = [];
  readonly #senderClaims: SenderClaim[] = [];
  readonly #sent: SentPayment[] = [];
  readonly #sentKeys = new Set<string>();
  readonly #unknownMints = new Set<Address>();
  #observedSlot = 0n;

  constructor(keys: ShieldedKeys, held: readonly WalletUtxo[]) {
    this.#keys = keys;
    this.#held = held;
    for (const note of held) {
      this.#known.add(bytesKey(note.outputContext.hash));
      this.#ownedNullifiers.add(bytesKey(note.nullifier));
    }
  }

  /** A claim stays with its note, a note found twice adds none. */
  absorb(viewingPublicKey: X25519PublicKey, read: BodyRead): void {
    for (const { utxo, nullifierPublicKey, body, claim, pending } of read.notes) {
      const { position } = body;
      const slot = body.tx.outputSlots[position];
      if (slot === undefined || this.isFound(slot.outputContext.hash)) continue;
      this.keep(
        {
          utxo,
          nullifierPublicKey,
          pending,
          outputContext: walletContext(slot),
          order: publication(body.tx, position),
        },
        viewingPublicKey,
        body.found,
      );
      if (claim !== undefined)
        this.#senderClaims.push(Object.freeze({ utxoHash: slot.outputContext.hash, claim }));
    }
    // A repeated row lists its payments again, the published output names each once.
    for (const payment of read.sent)
      if (unseen(this.#sentKeys, `${payment.txSignature}:${bytesKey(payment.utxoHash)}`))
        this.#sent.push(payment);
    for (const mint of read.unknownMints) this.unknownMint(mint);
  }

  get observedSlot(): bigint {
    return this.#observedSlot;
  }

  keep(note: Found, viewingPublicKey: X25519PublicKey, stream: AnonymousStream): void {
    this.#observedSlot = maxSlot(this.#observedSlot, note.order.slot);
    this.#known.add(bytesKey(note.outputContext.hash));
    this.#found.push(note);
    (stream.kind === "merge" ? this.#mergeStreams : this.#streams).push(
      Object.freeze({
        utxoHash: note.outputContext.hash,
        viewingPublicKey,
        nullifierPublicKey: note.nullifierPublicKey,
        stream,
      }),
    );
  }

  unknownMint(mint: Address): void {
    this.#unknownMints.add(mint);
  }

  isFound(hash: Bytes32): boolean {
    return this.#known.has(bytesKey(hash));
  }

  noteSpends(transactions: readonly IndexedShieldedTransaction[]): void {
    for (const tx of transactions)
      for (const nullifier of tx.nullifiers)
        if (this.#ownedNullifiers.has(bytesKey(nullifier)))
          this.#spent.set(bytesKey(nullifier), nullifier);
  }

  /** Derives the nullifiers of the found notes in publication order. */
  async assignNullifiers(context: RequestContext | undefined): Promise<void> {
    const found = this.#found;
    this.#found = [];
    const registered = found.filter((note) => !hasZeroNullifierKey(note));
    const derived =
      registered.length === 0
        ? []
        : await this.#keys.derive(
            registered.map(({ utxo, outputContext }) => ({
              kind: "nullifier" as const,
              utxoHash: outputContext.hash,
              blinding: utxo.blinding,
            })),
            context,
          );
    const nullifiers = new Map(answered(registered, derived));
    for (const note of found) {
      const { order, nullifierPublicKey: _nullifierPublicKey, ...fields } = note;
      const nullifier =
        nullifiers.get(note) ?? zeroKeyNullifier(fields.outputContext.hash, fields.utxo.blinding);
      const { pending, ...kept } = fields;
      this.#ownedNullifiers.add(bytesKey(nullifier));
      this.#newNullifiers.push(nullifier);
      (pending === true ? this.#pending : this.#decrypted).push({
        utxo: Object.freeze({ ...kept, nullifier, spent: false }),
        order,
      });
    }
  }

  takeNullifiers(): readonly Bytes32[] {
    const nullifiers = this.#newNullifiers;
    this.#newNullifiers = [];
    return nullifiers;
  }

  async recovery(context: RequestContext | undefined): Promise<AnonymousRecovery> {
    await this.assignNullifiers(context);
    this.#decrypted.sort((left, right) => compareOrder(left.order, right.order));
    this.#pending.sort((left, right) => compareOrder(left.order, right.order));
    return Object.freeze({
      utxos: this.#marked(this.#decrypted),
      pendingOpenings: this.#marked(this.#pending),
      spentNullifiers: Object.freeze(
        [...this.#decrypted, ...this.#pending, ...this.#held.map((utxo) => ({ utxo }))].flatMap(
          ({ utxo: { nullifier } }) => (this.#spent.has(bytesKey(nullifier)) ? [nullifier] : []),
        ),
      ),
      found: Object.freeze([...this.#streams, ...this.#mergeStreams]),
      senderClaims: Object.freeze([...this.#senderClaims]),
      sent: Object.freeze([...this.#sent]),
      unknownMints: Object.freeze([...this.#unknownMints]),
      unmatched: Object.freeze(this.#pending.map(({ utxo }) => utxo.outputContext.hash)),
    });
  }

  #marked(notes: readonly Decrypted[]): readonly WalletUtxo[] {
    return Object.freeze(
      notes.map(({ utxo }) =>
        Object.freeze({ ...utxo, spent: this.#spent.has(bytesKey(utxo.nullifier)) }),
      ),
    );
  }
}
const base58 = getBase58Encoder();

function hasZeroNullifierKey(note: Found): boolean {
  return equalBytes(note.nullifierPublicKey, ZERO_NULLIFIER_PK);
}

function walletContext(slot: OutputSlot): WalletUtxo["outputContext"] {
  return Object.freeze({
    hash: slot.outputContext.hash,
    tree: treeAddress(slot.outputContext.treeId),
    treeId: slot.outputContext.treeId,
    leafIndex: slot.outputContext.leafIndex,
  });
}

function publication(tx: IndexedShieldedTransaction, position: number): Publication {
  return { slot: tx.slot, signature: base58.encode(tx.txSignature), position };
}

/** Slot, then signature bytes, then output position. */
function compareOrder(left: Publication, right: Publication): number {
  if (left.slot !== right.slot) return left.slot < right.slot ? -1 : 1;
  const length = Math.min(left.signature.length, right.signature.length);
  for (let at = 0; at < length; at++) {
    const difference = (left.signature[at] ?? 0) - (right.signature[at] ?? 0);
    if (difference !== 0) return difference;
  }
  return left.signature.length - right.signature.length || left.position - right.position;
}

export function maxSlot(left: bigint, right: bigint): bigint {
  return left > right ? left : right;
}
