/**
 * Ring notes opened into a wallet through the core commitment check, Rust `zolana_ring_client::RingWalletSync`.
 *
 * Read `syncRingWallet` first. Each held key's `tagPosition` and stream counters resume the tag read beside the core sync's checkpoints, `AnonymousWallet._absorbSync` stores them with the notes.
 */
import { syncWallet, type SyncReport, type SyncWalletInput } from "@heliuslabs/spr";
import type { IndexerReader, IndexerRpcConfig } from "@heliuslabs/spr/client";
import {
  bytesKey,
  fetchSplAssetRegistrations,
  createIndexerRpcConfig,
  isCanonicalField,
  type CheckpointStream,
  type SyncCheckpointAdvances,
} from "@heliuslabs/spr/internal";
import { bootstrapSlotTag } from "../crypto/derivation/ring-stream.js";
import type { Address, Bytes32, RequestContext } from "@heliuslabs/spr/interface";
import type { ShieldedAddress, X25519PublicKey } from "@heliuslabs/spr/keypair";
import {
  assetField,
  decryptTransactions,
  Wallet,
  type AssetRegistry,
  type IndexedShieldedTransaction,
  type RingCodec,
  type RingOpening,
  type ShieldedKeys,
  type WalletUtxo,
} from "@heliuslabs/spr/transaction";

import { ringBodyOutput } from "../protocol/transaction/output.js";
import { openReply, type ReplyClaim } from "../protocol/codec/anonymous-delivery.js";
import { openRecipientBody } from "../crypto/derivation/blinding.js";
import { spendingTransactions } from "../operations/workflow/recover.js";
import { outputTreeId } from "../protocol/membership/trees.js";
import {
  isRingKeys,
  type RingDeliveryRequest,
  type RingKeys,
  type ViewTagRequest,
} from "./ring-keys.js";
import { isSmallOrder } from "../crypto/keys/x25519.js";
import { answered, decoded, unseen } from "./batch.js";
import { HalvingPageLimit, nextHistoryToken, TAGS_PER_REQUEST } from "../rpc/history.js";
import { isPendingNote } from "./ring-wallet.js";
import type { SenderClaim } from "./anonymous-sync.js";
import type { AnonymousWallet, FoundTag, SyncCommit } from "./anonymous-wallet.js";
import type { ViewingHistory, ViewingKeyEntry } from "./viewing-history.js";
import { RingError } from "../protocol/error.js";
import { defined } from "../protocol/defined.js";

export type RingNoteCodecInput = Readonly<{
  ringProgramId: Address;
  keys: RingKeys;
  assets: AssetRegistry;
}>;

/** Rust `RingNoteCodec`, each ring body tried under every held key. */
export function ringNoteCodec(input: RingNoteCodecInput): RingCodec {
  return claimingCodec(input, { claims: [], openedUnder: new WeakMap() });
}

/** What each slot a held key rebuilt leaves, core has not checked its note yet. */
type SlotReads = Readonly<{
  claims: SenderClaim[];
  openedUnder: WeakMap<OutputSlot, X25519PublicKey>;
}>;

function claimingCodec(input: RingNoteCodecInput, reads: SlotReads): RingCodec {
  return {
    ringProgramId: input.ringProgramId,
    open: async (event, context) => {
      const sites = [...sitesOf(event)].filter((site) => ringBodyOutput(site.slot.payload));
      const [first] = sites;
      const firstNullifier = event.nullifiers[0];
      if (
        first === undefined ||
        firstNullifier === undefined ||
        isSmallOrder(first.txViewingPublicKey) ||
        !isCanonicalField(firstNullifier)
      )
        return [];
      const { txViewingPublicKey, salt } = first;
      // One key's requests run together, the holder agrees once per key.
      const requests = input.keys.viewingPublicKeys().flatMap((viewingPublicKey) =>
        sites.map((site): Readonly<{ site: Site; request: RingDeliveryRequest }> => ({
          site,
          request: {
            viewingPublicKey,
            txViewingPublicKey,
            salt,
            outputIndex: site.slotIndex,
            firstNullifier,
          },
        })),
      );
      if (requests.length === 0) return [];
      const secrets = await input.keys.ringDeliverySecrets(
        requests.map(({ request }) => request),
        context,
      );
      const openings: RingOpening[] = [];
      for (const [{ site, request }, secret] of answered(requests, secrets)) {
        const opened = openedSlot(input, site, secret);
        if (opened === undefined) continue;
        const { utxo, nullifierPublicKey, dataHash, ringDataHash } = opened.note;
        reads.openedUnder.set(site.slot, request.viewingPublicKey);
        if (opened.claim !== undefined)
          reads.claims.push({ utxoHash: site.slot.outputContext.hash, claim: opened.claim });
        openings.push({
          slotIndex: site.slotIndex,
          nullifierPublicKey,
          mint: utxo.asset,
          amount: utxo.amount,
          blinding: utxo.blinding,
          data: utxo.data,
          ...defined({ dataHash, ringDataHash }),
        });
      }
      return openings;
    },
  };
}

/** `undefined` without a reply or on bytes that never decode. */
export function claimOf(reply: Uint8Array, secret: Bytes32): ReplyClaim | undefined {
  return reply.length === 0 ? undefined : decoded(() => openReply(secret, reply));
}

function* sitesOf(tx: IndexedShieldedTransaction): Generator<Site> {
  const { ringProgramId: ring, txViewingPublicKey, salt } = tx;
  if (ring === undefined || txViewingPublicKey === undefined || salt === undefined) return;
  for (const [slotIndex, slot] of tx.outputSlots.entries()) {
    yield { tx, ring, txViewingPublicKey, salt, slotIndex, slot };
  }
}

export type RingSyncInput = Omit<SyncWalletInput, "wallet" | "keys"> &
  Readonly<{ wallet: AnonymousWallet; keys: RingKeys; client: RingSyncClient }>;

/** Notes, positions, claims and merge retries commit together after every read succeeds, Rust `RingWalletSync::sync`. */
export async function syncRingWallet(
  input: RingSyncInput,
  context?: RequestContext,
): Promise<SyncReport> {
  const { wallet: ring, keys } = input;
  const { wallet } = ring;
  // A core sync marks a held note spent, the ring change of that spend then opens to no later sync.
  if (!isRingKeys(keys)) throw new RingError("RING_KEYS_REQUIRED", { details: { reason: "sync" } });
  return wallet._withSyncLock(async () => {
    const revision = wallet._revision;
    // 1. The core sync resumes from its checkpoints on a copy, nothing reaches the wallet before step 4.
    const staging = wallet._clone();
    const reads = new RingReads(input, staging, context);
    const report = await syncWallet({ ...input, wallet: staging, client: reads.client }, context);
    // 2. Core opens the rows under each held key's listed tags past its position.
    const tagged = await reads.tagged(keys, ring.history());
    if (ring.pendingOpenings().some((note) => isPendingNote(staging.registry, note)))
      await reads.refreshRegistry();
    // 3. The core sync read the spends of held notes. A note new to the wallet or promoted and a stored retry still need theirs.
    // A merge output under an unregistered mint opens and waits as pending, a TS sync adds no retry and rereads a stored one until the spend row returns.
    const held = new Set([...staging.utxos(), ...ring.pendingOpenings()].map(outputKey));
    const fresh = reads.notes.filter((note) => unseen(held, outputKey(note)));
    const promoted = ring
      .pendingOpenings()
      .filter((note) => !isPendingNote(staging.registry, note));
    const retries = ring._mergeRetries();
    const read = new Set<string>();
    const owner = keys.address();
    const budget = { remaining: Number.MAX_SAFE_INTEGER };
    const chunkSize = input.config?.queryChunk ?? 64;
    let spends = reads.spendRows();
    let unread = [...[...fresh, ...promoted].map((note) => note.nullifier), ...retries];
    for (;;) {
      for (let offset = 0; offset < unread.length; offset += chunkSize)
        spends = [
          ...spends,
          ...(await spendingTransactions(
            reads.client,
            unread.slice(offset, offset + chunkSize),
            budget,
            context,
          )),
        ];
      const rows = spends.filter((row) => unseen(read, eventKey(row)));
      const merged = await openRingMerges(
        { keys, owner, assets: staging.registry, spends: rows },
        context,
      );
      // A spend can return change no listed tag names, under a random tag or from a delegate's move.
      const returned = await reads.open(keys, rows);
      const next = [...merged.utxos, ...returned].filter((note) => unseen(held, outputKey(note)));
      if (next.length === 0) break;
      fresh.push(...next);
      spends = [];
      unread = next.map((note) => note.nullifier);
    }
    // 4. The core rows, the ring notes, the positions, the claims and the retries land in one commit.
    const published = new Set(reads.spentNullifiers().map(bytesKey));
    const initial = new Set(wallet.utxos().map(outputKey));
    const opened = new Set(fresh.map(outputKey));
    let storedUtxos = 0;
    ring._absorbSync(
      {
        notes: fresh,
        spentNullifiers: reads.spentNullifiers(),
        ...tagged,
        senderClaims: reads.claims.filter((claim) => opened.has(bytesKey(claim.utxoHash))),
        retried: retries.filter((nullifier) => published.has(bytesKey(nullifier))),
      },
      staging,
      (commit) => {
        wallet._commitSync(
          {
            ...staging._state(),
            ...commit,
            lastSynced: staging.lastSynced,
            checkpoints: reads.checkpoints(),
            registryAdditions: staging.registry
              .entries()
              .map(([assetId, mint]) => ({ assetId, mint })),
          },
          revision,
        );
        storedUtxos = commit.utxos.filter((note) => !initial.has(outputKey(note))).length;
      },
    );
    const unregisteredRingNotes = ring
      .pendingOpenings()
      .filter((note) => opened.has(outputKey(note)));
    return Object.freeze({
      ...report,
      storedUtxos,
      unknownAssetFields: withAssetFields(report.unknownAssetFields, unregisteredRingNotes),
      unregisteredRingNotes,
    });
  });
}

/** Distinct and sorted as core reports them. */
function withAssetFields(
  fields: readonly Bytes32[],
  notes: readonly WalletUtxo[],
): readonly Bytes32[] {
  const byKey = new Map(
    [...fields, ...notes.map((note) => assetField(note.utxo.asset))].map(
      (field) => [bytesKey(field), field] as const,
    ),
  );
  return [...byKey].sort(([left], [right]) => (left < right ? -1 : 1)).map(([, field]) => field);
}

export type RingSyncClient = SyncWalletInput["client"] &
  Pick<IndexerReader, "getShieldedTransactionsByNullifiers" | "getShieldedTransactionsByTags">;

interface Site {
  readonly tx: IndexedShieldedTransaction;
  readonly ring: NonNullable<IndexedShieldedTransaction["ringProgramId"]>;
  readonly txViewingPublicKey: X25519PublicKey;
  readonly salt: NonNullable<IndexedShieldedTransaction["salt"]>;
  readonly slotIndex: number;
  readonly slot: OutputSlot;
}

type OutputSlot = IndexedShieldedTransaction["outputSlots"][number];

function outputKey(note: WalletUtxo): string {
  return bytesKey(note.outputContext.hash);
}

/** One transaction can emit several relevant events, only its signature and event index name one. */
function eventKey(row: IndexedShieldedTransaction): string {
  return `${row.txSignature}:${String(row.eventIndex)}`;
}

/** A merge publishes no transaction key or salt. */
function isRingMergeRow(tx: IndexedShieldedTransaction): boolean {
  return (
    tx.ringProgramId !== undefined &&
    !tx.proofless &&
    tx.txViewingPublicKey === undefined &&
    tx.salt === undefined
  );
}

/** The ring merges among `spends` and the notes they open to `owner`, Rust `RingMerges`. */
export async function openRingMerges(
  input: Readonly<{
    keys: ShieldedKeys;
    owner: ShieldedAddress;
    assets: AssetRegistry;
    spends: readonly IndexedShieldedTransaction[];
  }>,
  context: RequestContext | undefined,
): Promise<
  Readonly<{
    merges: readonly IndexedShieldedTransaction[];
    utxos: readonly WalletUtxo[];
  }>
> {
  const merges = input.spends.filter(isRingMergeRow);
  if (merges.length === 0) return { merges, utxos: [] };
  const { utxos } = await openedByCore({ ...input, transactions: merges }, context);
  return { merges, utxos };
}

/** Core stores what opens on a scratch wallet, the other nullifiers of `transactions` never reach the wallet. */
async function openedByCore(
  input: Readonly<{
    keys: ShieldedKeys;
    owner: ShieldedAddress;
    assets: AssetRegistry;
    transactions: readonly IndexedShieldedTransaction[];
    ring?: RingCodec;
  }>,
  context: RequestContext | undefined,
): Promise<Readonly<{ report: SyncReport; utxos: readonly WalletUtxo[] }>> {
  const { keys, transactions, ring } = input;
  const opened = new Wallet({ identity: input.owner, registry: input.assets });
  const report = await decryptTransactions({
    wallet: opened,
    keys,
    transactions,
    ...defined({ ring }),
    context,
  });
  return { report, utxos: opened._state().utxos };
}

/** Wipes `slotSecret`, `undefined` unless the body opens to the published commitment. */
function openedSlot(
  input: Readonly<{ keys: RingKeys; assets: AssetRegistry }>,
  site: Site,
  slotSecret: Bytes32,
):
  | Readonly<{ note: NonNullable<ReturnType<typeof openRecipientBody>>; claim?: ReplyClaim }>
  | undefined {
  const parts = ringBodyOutput(site.slot.payload);
  try {
    if (parts === undefined) return undefined;
    const note = decoded(() =>
      openRecipientBody(
        {
          owner: input.keys.address(),
          ring: site.ring,
          treeId: outputTreeId(site.slot.outputContext),
          commitment: site.slot.outputContext.hash,
          assets: input.assets,
        },
        slotSecret,
        parts.body,
      ),
    );
    if (note === undefined) return undefined;
    const claim = claimOf(parts.reply, slotSecret);
    return claim === undefined ? { note } : { note, claim };
  } finally {
    slotSecret.fill(0);
  }
}

type TagRead = Pick<SyncCommit, "tagPosition" | "foundTags">;

interface TagGroup {
  /** `null` reads from the first indexed block. */
  readonly position: bigint | null;
  readonly tags: Bytes32[];
  /** Full bootstrap tags, read on the proofless stream. */
  readonly deposits: Bytes32[];
}

/** Rust `Listing`. */
class TagListing {
  readonly #streamTags = new Map<string, FoundTag>();
  readonly #bounds = new Map<string, bigint>();
  #first = true;

  /** An index within the window of a found one was read through its key's position, any other tag was never read. */
  async list(
    keys: RingKeys,
    entries: readonly ViewingKeyEntry[],
    context: RequestContext | undefined,
  ): Promise<readonly TagGroup[]> {
    const first = this.#first;
    this.#first = false;
    const groups = new Map<bigint | null, TagGroup>();
    const group = (position: bigint | null): TagGroup => {
      const held = groups.get(position) ?? { position, tags: [], deposits: [] };
      groups.set(position, held);
      return held;
    };
    const asked: (ViewTagRequest & Readonly<{ position: bigint | null }>)[] = [];
    for (const entry of entries) {
      const { viewingPublicKey } = entry;
      if (first) {
        const bootstrap = group(entry.tagPosition);
        bootstrap.tags.push(bootstrapSlotTag(viewingPublicKey.u()));
        bootstrap.deposits.push(viewingPublicKey.u());
      }
      for (const [stream, counter] of entry._streams()) {
        const bound = `${bytesKey(viewingPublicKey.toBytes())}:${stream}`;
        const start = this.#bounds.get(bound) ?? 0n;
        const { end } = counter._listed();
        this.#bounds.set(bound, end);
        // A later round lists only tags no sync read.
        const readBefore = first ? counter._readBefore().end : 0n;
        const upper = readBefore < end ? readBefore : end;
        const split = upper > start ? upper : start;
        const parts = [
          { indices: { start, end: split }, position: entry.tagPosition },
          { indices: { start: split, end }, position: null },
        ];
        for (const { indices, position } of parts)
          if (indices.start < indices.end)
            asked.push({ viewingPublicKey, stream, indices, position });
      }
    }
    const answers =
      asked.length === 0
        ? []
        : await keys.viewTags(
            asked.map(({ viewingPublicKey, stream, indices }) => ({
              viewingPublicKey,
              stream,
              indices,
            })),
            context,
          );
    for (const [{ viewingPublicKey, stream, indices, position }, tags] of answered(
      asked,
      answers,
    )) {
      const indexes = Array.from(
        { length: Number(indices.end - indices.start) },
        (_, offset) => indices.start + BigInt(offset),
      );
      for (const [index, tag] of answered(indexes, tags)) {
        this.#streamTags.set(bytesKey(tag), { viewingPublicKey, stream, index });
        group(position).tags.push(tag);
      }
    }
    return [...groups.values()].sort(({ position: left }, { position: right }) =>
      left === null ? -1 : right === null ? 1 : left < right ? -1 : left > right ? 1 : 0,
    );
  }

  /** The stream tags `row` publishes on a slot opened under the key each was listed for. */
  found(
    row: IndexedShieldedTransaction,
    openedUnder: WeakMap<OutputSlot, X25519PublicKey>,
  ): readonly FoundTag[] {
    return row.outputSlots.flatMap((slot) => {
      const listed = this.#streamTags.get(bytesKey(slot.viewTag));
      return listed !== undefined && openedUnder.get(slot)?.equals(listed.viewingPublicKey) === true
        ? [listed]
        : [];
    });
  }
}

class RingReads {
  readonly client: RingSyncClient;
  readonly notes: WalletUtxo[] = [];
  /** Unchecked until its note is among `notes`. */
  readonly claims: SenderClaim[] = [];
  readonly #openedUnder = new WeakMap<OutputSlot, X25519PublicKey>();
  readonly #staging: Wallet;
  readonly #context: RequestContext | undefined;
  readonly #rpcConfig: IndexerRpcConfig;
  readonly #pageLimit: HalvingPageLimit;
  readonly #spends: IndexedShieldedTransaction[] = [];
  #refreshed = false;

  constructor(input: RingSyncInput, staging: Wallet, context: RequestContext | undefined) {
    const { client, config } = input;
    this.#staging = staging;
    this.#context = context;
    this.#rpcConfig = createIndexerRpcConfig(
      config?.retry ?? client.indexerConfig?.poll,
      config?.requireSlot ?? client.indexerConfig?.requireSlot,
    );
    const limit = config?.pageLimit ?? 1_000;
    this.#pageLimit = new HalvingPageLimit(limit);
    this.client = {
      ...defined({
        solanaRpc: client.solanaRpc,
        commitment: client.commitment,
        indexerConfig: client.indexerConfig,
      }),
      getEncryptedUtxosByTags: (request, rpcConfig, context) =>
        client.getEncryptedUtxosByTags(request, rpcConfig, context),
      getShieldedTransactionsByTags: (request, rpcConfig, context) =>
        client.getShieldedTransactionsByTags(request, rpcConfig, context),
      // Every spend read, the core sync's and the ring's, marks the notes it names.
      getShieldedTransactionsByNullifiers: async (request, rpcConfig, context) => {
        const page = await client.getShieldedTransactionsByNullifiers(
          { limit, ...request },
          rpcConfig ?? this.#rpcConfig,
          context,
        );
        this.#spends.push(...page.data.flatMap((item) => item.events));
        return page;
      },
    };
  }

  /**
   * Rust `read_tags`.
   *
   * Keys at one position share a read, an unread tag reads from the first indexed block.
   * Every read key moves to the lowest terminal slot, one shared position keeps the next sync at one read.
   */
  async tagged(keys: RingKeys, history: ViewingHistory): Promise<TagRead> {
    // 1. A held key with no entry would read nothing and report nothing missed.
    const held = keys.viewingPublicKeys();
    if (held.some((key) => history.entry(key) === undefined))
      throw new RingError("RING_HELD_KEY_NOT_IN_HISTORY");
    const holds = (entry: ViewingKeyEntry): boolean =>
      held.some((key) => key.equals(entry.viewingPublicKey));
    const listing = new TagListing();
    const seen = new Set<string>();
    const foundTags: FoundTag[] = [];
    let counted = history;
    let covered: bigint | undefined;
    for (;;) {
      // 2. One read per group and stream, a moved counter lists more next round.
      const groups = await listing.list(keys, counted.entries.filter(holds), this.#context);
      if (groups.length === 0) break;
      const read: IndexedShieldedTransaction[] = [];
      for (const { position, tags, deposits } of groups)
        for (const slot of [
          await this.#read("transactions", tags, position, read),
          await this.#read("proofless", deposits, position, read),
        ])
          if (slot !== undefined)
            covered = covered === undefined || slot < covered ? slot : covered;
      // 3. A tag proves no ownership, each row opens once here and a row that opens nothing moves
      // no counter.
      const fresh = read.filter((row) => unseen(seen, eventKey(row)));
      await this.open(keys, fresh);
      for (const found of fresh.flatMap((row) => listing.found(row, this.#openedUnder))) {
        counted = counted._withFound(found.viewingPublicKey, found.stream, found.index);
        foundTags.push(found);
      }
    }
    return {
      ...(covered === undefined ? {} : { tagPosition: { keys: held, afterSlot: covered } }),
      foundTags,
    };
  }

  /** `true` when the first refresh of the sync registered a mint. */
  async refreshRegistry(): Promise<boolean> {
    const { solanaRpc, commitment } = this.client;
    if (this.#refreshed || solanaRpc === undefined || commitment === undefined) return false;
    this.#refreshed = true;
    let registered = false;
    for (const { assetId, mint } of await fetchSplAssetRegistrations(
      { solanaRpc, commitment },
      this.#context,
    ))
      registered = this.#staging.ensureAsset(assetId, mint) || registered;
    return registered;
  }

  /** Every row a spend read returned so far. */
  spendRows(): readonly IndexedShieldedTransaction[] {
    return [...this.#spends];
  }

  spentNullifiers(): readonly Bytes32[] {
    return this.#spends.flatMap((transaction) => transaction.nullifiers);
  }

  checkpoints(): SyncCheckpointAdvances {
    const staged = (stream: CheckpointStream) => new Map(this.#staging._checkpointEntries(stream));
    return {
      transactions: staged("transactions"),
      proofless: staged("proofless"),
      nullifiers: staged("nullifiers"),
    };
  }

  /** The ring notes `rows` open to the holder, each kept in `notes`. */
  async open(
    keys: RingKeys,
    rows: readonly IndexedShieldedTransaction[],
  ): Promise<readonly WalletUtxo[]> {
    const events = rows.filter((row) => !isRingMergeRow(row));
    const notes: WalletUtxo[] = [];
    for (const ringProgramId of new Set(events.flatMap((row) => row.ringProgramId ?? []))) {
      const decrypt = () =>
        openedByCore(
          {
            keys,
            owner: keys.address(),
            assets: this.#staging.registry,
            transactions: events.filter((row) => row.ringProgramId === ringProgramId),
            ring: claimingCodec(
              { ringProgramId, keys, assets: this.#staging.registry },
              { claims: this.claims, openedUnder: this.#openedUnder },
            ),
          },
          this.#context,
        );
      let opened = await decrypt();
      if (opened.report.unregisteredRingNotes.length > 0 && (await this.refreshRegistry()))
        opened = await decrypt();
      notes.push(...opened.utxos, ...opened.report.unregisteredRingNotes);
    }
    this.notes.push(...notes);
    return notes;
  }

  /** The lowest terminal snapshot slot covers every match of `tags` above `after`, `undefined` with no tag. */
  async #read(
    stream: "transactions" | "proofless",
    tags: readonly Bytes32[],
    after: bigint | null,
    rows: IndexedShieldedTransaction[],
  ): Promise<bigint | undefined> {
    let covered: bigint | undefined;
    for (let start = 0; start < tags.length; start += TAGS_PER_REQUEST) {
      const slot = await this.#readChunk(
        stream,
        tags.slice(start, start + TAGS_PER_REQUEST),
        after,
        rows,
      );
      covered = covered === undefined || slot < covered ? slot : covered;
    }
    return covered;
  }

  async #readChunk(
    stream: "transactions" | "proofless",
    tags: readonly Bytes32[],
    after: bigint | null,
    rows: IndexedShieldedTransaction[],
  ): Promise<bigint> {
    let token: string | undefined;
    for (;;) {
      const asked = {
        tags,
        ...(after === null ? {} : { filters: { slot: { gt: after } } }),
        ...(token === undefined ? {} : { paginationToken: token }),
      };
      const page =
        stream === "transactions"
          ? await this.#pageLimit.read(async (limit) => {
              const read = await this.client.getShieldedTransactionsByTags(
                { ...asked, limit },
                this.#rpcConfig,
                this.#context,
              );
              rows.push(...read.data.flatMap((item) => item.events));
              return read;
            })
          : await this.#pageLimit.read(async (limit) => {
              const read = await this.client.getEncryptedUtxosByTags(
                { ...asked, limit },
                this.#rpcConfig,
                this.#context,
              );
              rows.push(...prooflessRows(read));
              return read;
            });
      const next = nextHistoryToken(token, page);
      if (next === undefined) return page.context.slot;
      token = next;
    }
  }
}

/** A match carrying a transaction key or salt is a transfer slot, not a deposit. */
function prooflessRows(
  page: Awaited<ReturnType<RingSyncClient["getEncryptedUtxosByTags"]>>,
): readonly IndexedShieldedTransaction[] {
  return page.data
    .flatMap((item) => item.outputs)
    .filter((match) => match.txViewingPk === undefined && match.salt === undefined)
    .map((match) => ({
      slot: match.slot,
      txSignature: match.txSignature,
      outputSlots: [match.outputSlot],
      messages: [],
      nullifiers: [],
      proofless: true,
    }));
}
