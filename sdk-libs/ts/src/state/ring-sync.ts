/**
 * Ring notes opened into a wallet through the core commitment check, Rust `zolana_ring_client::RingWalletSync`.
 *
 * Read `syncRingWallet` first. Each held key's `tagPosition` and ring positions resume the ring reads beside the core sync's checkpoints, `AnonymousWallet._absorbSync` stores them with the notes.
 */
import { syncWallet, type SyncReport, type SyncWalletInput } from "@heliuslabs/spr";
import type { IndexerReader, IndexerRpcConfig } from "@heliuslabs/spr/client";
import {
  bytesKey,
  equalBytes,
  fetchSplAssetRegistrations,
  createIndexerRpcConfig,
  isCanonicalField,
  isZero,
  type CheckpointStream,
  type SyncCheckpointAdvances,
} from "@heliuslabs/spr/internal";
import { confidentialSlotTag } from "../crypto/derivation/ring-stream.js";
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

import { ringBodyOutput, senderBundleOutputBody } from "../protocol/transaction/output.js";
import {
  openReply,
  SENDER_BUNDLE_LEN,
  type ReplyClaim,
} from "../protocol/codec/anonymous-delivery.js";
import { openRecipientNote } from "../crypto/derivation/blinding.js";
import { openOutputSlot } from "../crypto/audit/ring-disclosure.js";
import { spendingTransactions } from "../operations/workflow/recover.js";
import { outputTreeId } from "../protocol/membership/trees.js";
import {
  isRingKeys,
  type RingDeliveryRequest,
  type RingKeys,
  type RingTagFrame,
  type RingTagProbe,
} from "./ring-keys.js";
import { isSmallOrder } from "../crypto/keys/x25519.js";
import { answered, decoded, unseen } from "./batch.js";
import { HalvingPageLimit, nextHistoryToken, readRingPages, scanLimits } from "../rpc/history.js";
import { isPendingNote } from "./ring-wallet.js";
import type { SenderClaim } from "./anonymous-sync.js";
import type { AnonymousWallet, SyncCommit } from "./anonymous-wallet.js";
import type { RingRead, ViewingHistory } from "./viewing-history.js";
import { RingError } from "../protocol/error.js";

export type RingNoteCodecInput = Readonly<{
  ringProgramId: Address;
  keys: RingKeys;
  assets: AssetRegistry;
}>;

/** Rust `RingNoteCodec`, one holder read per event and one more for an own event's change. */
export function ringNoteCodec(input: RingNoteCodecInput): RingCodec {
  return claimingCodec(input, []);
}

/** Every reply under a slot the holder opened lands in `claims`, core has not checked its note yet. */
function claimingCodec(input: RingNoteCodecInput, claims: SenderClaim[]): RingCodec {
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
      const bundle = event.messages.find(
        (message) => senderBundleOutputBody(message.data)?.length === SENDER_BUNDLE_LEN,
      );
      const tagged = sites.filter((site) => !isZero(site.slot.viewTag));
      const frames: RingTagFrame[] = input.keys
        .viewingPublicKeys()
        .map((viewingPublicKey): RingTagFrame => {
          const confidential = confidentialSlotTag(viewingPublicKey.u());
          const probes: RingTagProbe[] = tagged.map((site) => ({
            kind: equalBytes(site.slot.viewTag, confidential) ? "confidential" : "recipient",
            outputIndex: site.slotIndex,
            tag: site.slot.viewTag,
          }));
          if (bundle !== undefined)
            probes.push({
              kind: "senderBundle",
              outputIndex: event.outputSlots.length,
              tag: bundle.viewTag,
            });
          return { viewingPublicKey, txViewingPublicKey, salt, firstNullifier, probes };
        })
        .filter((frame) => frame.probes.length > 0);
      if (frames.length === 0) return [];
      const secrets: (readonly [number, Bytes32])[] = [];
      try {
        return await openedFrames(input, { sites, frames, secrets, claims }, context);
      } finally {
        for (const [, secret] of secrets) secret.fill(0);
      }
    },
  };
}

/** Gathers each slot secret into `read.secrets`, the caller wipes them. */
async function openedFrames(
  input: RingNoteCodecInput,
  read: Readonly<{
    sites: readonly Site[];
    frames: readonly RingTagFrame[];
    secrets: (readonly [number, Bytes32])[];
    claims: SenderClaim[];
  }>,
  context: RequestContext | undefined,
): Promise<readonly RingOpening[]> {
  const { sites, frames, secrets, claims } = read;
  const change: RingDeliveryRequest[] = [];
  const answers = await input.keys.ringTagMatches(frames, context);
  for (const [frame, probeAnswers] of answered(frames, answers)) {
    for (const [probe, secret] of answered(frame.probes, probeAnswers)) {
      if (secret === undefined) continue;
      if (probe.kind !== "senderBundle") {
        secrets.push([probe.outputIndex, secret]);
        claims.push(...replyClaims(sites, probe.outputIndex, secret));
        continue;
      }
      secret.fill(0);
      // A matched bundle marks an own event, its change carries tag zero.
      for (const site of sites.filter((candidate) => isZero(candidate.slot.viewTag)))
        change.push({ ...frame, outputIndex: site.slotIndex });
    }
  }
  if (change.length > 0)
    for (const [request, secret] of answered(
      change,
      await input.keys.ringDeliverySecrets(change, context),
    ))
      secrets.push([request.outputIndex, secret]);
  const openings: RingOpening[] = [];
  for (const [slotIndex, secret] of secrets) {
    const site = sites.find((candidate) => candidate.slotIndex === slotIndex);
    const note = site === undefined ? undefined : openedNote(input, site, secret);
    if (note === undefined) continue;
    const { utxo, nullifierPublicKey, dataHash, ringDataHash } = note;
    openings.push({
      slotIndex,
      nullifierPublicKey,
      mint: utxo.asset,
      amount: utxo.amount,
      blinding: utxo.blinding,
      data: utxo.data,
      ...(dataHash === undefined ? {} : { dataHash }),
      ...(ringDataHash === undefined ? {} : { ringDataHash }),
    });
  }
  return openings;
}

function replyClaims(
  sites: readonly Site[],
  slotIndex: number,
  secret: Bytes32,
): readonly SenderClaim[] {
  return sites.flatMap((site) => {
    const reply =
      site.slotIndex === slotIndex ? ringBodyOutput(site.slot.payload)?.reply : undefined;
    const claim = reply === undefined ? undefined : claimOf(reply, secret);
    return claim === undefined ? [] : [{ utxoHash: site.slot.outputContext.hash, claim }];
  });
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

export type RingSyncInput = Omit<SyncWalletInput, "wallet"> &
  Readonly<{ wallet: AnonymousWallet; client: RingSyncClient }>;

/** Notes, positions, claims and merge retries commit together after every read succeeds, Rust `RingWalletSync::sync`. */
export async function syncRingWallet(
  input: RingSyncInput,
  context?: RequestContext,
): Promise<SyncReport> {
  const { wallet: ring, keys } = input;
  const { wallet } = ring;
  return wallet._withSyncLock(async () => {
    const revision = wallet._revision;
    // 1. The core sync resumes from its checkpoints on a copy, nothing reaches the wallet before step 4.
    const staging = wallet._clone();
    const reads = new RingReads(input, staging, context);
    const report = await syncWallet({ ...input, wallet: staging, client: reads.client }, context);
    // 2. Core opens what each held key reads past its positions, the rows under its slot tags and every row of each ring it reads.
    // Only ring keys open a body, a sync without them leaves every position for one with them.
    let tagPosition: SyncCommit["tagPosition"];
    let ringPositions: SyncCommit["ringPositions"] = [];
    if (isRingKeys(keys)) {
      tagPosition = await reads.tagged(keys, ring.history());
      ringPositions = await reads.rings(
        keys,
        ring.history()._ringReads(keys.viewingPublicKeys(), false),
      );
    }
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
      const merges = spends.filter(
        (row) =>
          isRingMergeRow(row) && unseen(read, `${row.txSignature}:${String(row.eventIndex)}`),
      );
      const merged = await openRingMerges(
        { keys, owner, assets: staging.registry, spends: merges },
        context,
      );
      const next = merged.utxos.filter((note) => unseen(held, outputKey(note)));
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
        ...(tagPosition === undefined ? {} : { tagPosition }),
        ringPositions,
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
  readonly slot: IndexedShieldedTransaction["outputSlots"][number];
}

function outputKey(note: WalletUtxo): string {
  return bytesKey(note.outputContext.hash);
}

/** A merge publishes no transaction key or salt. */
export function isRingMergeRow(tx: IndexedShieldedTransaction): boolean {
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
    ...(ring === undefined ? {} : { ring }),
    context,
  });
  return { report, utxos: opened._state().utxos };
}

/** Wipes `slotSecret`, `undefined` unless the body opens to the published commitment. */
function openedNote(
  input: Readonly<{ keys: RingKeys; assets: AssetRegistry }>,
  site: Site,
  slotSecret: Bytes32,
): ReturnType<typeof openRecipientNote> {
  const parts = ringBodyOutput(site.slot.payload);
  try {
    return parts === undefined
      ? undefined
      : decoded(() =>
          openRecipientNote(
            {
              owner: input.keys.address(),
              ring: site.ring,
              treeId: outputTreeId(site.slot.outputContext),
              commitment: site.slot.outputContext.hash,
              assets: input.assets,
            },
            openOutputSlot(slotSecret, parts.body),
          ),
        );
  } finally {
    slotSecret.fill(0);
  }
}

class RingReads {
  readonly client: RingSyncClient;
  readonly notes: WalletUtxo[] = [];
  /** Unchecked until its note is among `notes`. */
  readonly claims: SenderClaim[] = [];
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
      ...(client.solanaRpc === undefined ? {} : { solanaRpc: client.solanaRpc }),
      ...(client.commitment === undefined ? {} : { commitment: client.commitment }),
      ...(client.indexerConfig === undefined ? {} : { indexerConfig: client.indexerConfig }),
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
   * Each held key's slot tag and deposits above the key's tag position, Rust `read_tags`.
   *
   * Keys at one position share a read, an unread key reads from the first indexed block.
   * Every read key moves to the lowest terminal slot, one shared position keeps the next sync at one read.
   */
  async tagged(keys: RingKeys, history: ViewingHistory): Promise<SyncCommit["tagPosition"]> {
    // 1. A held key with no entry would read nothing and report nothing missed.
    const held = keys.viewingPublicKeys();
    const groups = new Map<bigint | null, X25519PublicKey[]>();
    for (const key of held) {
      const entry = history.entry(key);
      if (entry === undefined) throw new RingError("RING_HELD_KEY_NOT_IN_HISTORY");
      groups.set(entry.tagPosition, [...(groups.get(entry.tagPosition) ?? []), key]);
    }
    // 2. One read per group and stream, an unread group first.
    const ordered = [...groups].sort(([left], [right]) =>
      left === null ? -1 : right === null ? 1 : left < right ? -1 : left > right ? 1 : 0,
    );
    const rows: IndexedShieldedTransaction[] = [];
    let covered: bigint | undefined;
    for (const [position, members] of ordered) {
      const slotTags = members.map((key) => confidentialSlotTag(key.u()));
      const reached = [
        await this.#read("transactions", slotTags, position, rows),
        await this.#read(
          "proofless",
          members.map((key) => key.u()),
          position,
          rows,
        ),
      ];
      for (const slot of reached)
        covered = covered === undefined || slot < covered ? slot : covered;
    }
    await this.#open(keys, rows);
    return covered === undefined ? undefined : { keys: held, afterSlot: covered };
  }

  /** Each read to its terminal page, every page opened before the next arrives. */
  async rings(keys: RingKeys, reads: readonly RingRead[]): Promise<SyncCommit["ringPositions"]> {
    const limits = scanLimits({
      pageSize: this.#pageLimit.value,
      maxPages: Number.MAX_SAFE_INTEGER,
    });
    const positions: SyncCommit["ringPositions"][number][] = [];
    for (const read of reads) {
      const { checkpoint } = await readRingPages(
        {
          client: this.client,
          ringProgramId: read.ring,
          limits,
          ...(read.afterSlot === null ? {} : { afterSlot: read.afterSlot }),
          config: this.#rpcConfig,
        },
        (rows) => this.#open(keys, rows),
        this.#context,
      );
      if (checkpoint !== undefined) positions.push({ ...read, afterSlot: checkpoint });
    }
    return positions;
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

  async #open(keys: RingKeys, rows: readonly IndexedShieldedTransaction[]): Promise<void> {
    const events = rows.filter((row) => !isRingMergeRow(row));
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
              this.claims,
            ),
          },
          this.#context,
        );
      let opened = await decrypt();
      if (opened.report.unregisteredRingNotes.length > 0 && (await this.refreshRegistry()))
        opened = await decrypt();
      this.notes.push(...opened.utxos, ...opened.report.unregisteredRingNotes);
    }
  }

  /** Every row of `tags` above `after`, the terminal page's snapshot slot covers every match at or below it. */
  async #read(
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
