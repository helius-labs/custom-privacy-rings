import { bytesToHex, hexToBytes } from "@noble/hashes/utils.js";
import {
  ClientError,
  DEFAULT_INDEXER_POLL_CONFIG,
  type IndexerRpcConfig,
} from "@heliuslabs/spr/client";
import { bytesKey } from "@heliuslabs/spr/internal";
import type { Address, Bytes32, RequestContext } from "@heliuslabs/spr/interface";
import { X25519PublicKey, type ShieldedAddress } from "@heliuslabs/spr/keypair";
import type { IndexedShieldedTransaction } from "@heliuslabs/spr/transaction";
import { ringBodyOutput } from "../../protocol/transaction/output.js";
import { HalvingPageLimit, readRingPages, TAGS_PER_REQUEST } from "../../rpc/history.js";
import {
  RECOVERY_DIAGNOSTIC_SAMPLES,
  type RecoverySession,
  type RecoveryTask,
  type RecoveryPage,
} from "../recovery-session.js";
import { unseen } from "../batch.js";
import { RingError } from "../../protocol/error.js";
import { ViewingHistory, type ViewingKeyEntry } from "../viewing-history.js";
import {
  type SyncFields,
  type AnonymousSyncClient,
  RecoveryPaused,
  type AnonymousScan,
  issuedScans,
  type BodyReads,
} from "./scan-result.js";
import { NoteLedger, maxSlot, keepRingMerges, openMerges, type MergeReads } from "./note-ledger.js";
import {
  BodyRead,
  bodySecrets,
  ownTransactionKeys,
  readBodies,
  ringBodies,
  type Body,
} from "./body-read.js";
import { defined } from "../../protocol/defined.js";

export class Scan {
  readonly #unopened = new Map<string, Bytes32>();
  readonly #sync: SyncFields;
  readonly #client: AnonymousSyncClient;
  readonly #context: RequestContext | undefined;
  readonly #address: ShieldedAddress;
  readonly #ledger: NoteLedger;
  /** Held notes stay out of the ledger, a merge output among them is not found again. */
  readonly #heldHashes: ReadonlySet<string>;
  readonly #originalHistory: ViewingHistory;
  readonly #pageLimit = new HalvingPageLimit(TAGS_PER_REQUEST);
  #history: ViewingHistory;
  #status: "Paused" | "Complete" = "Complete";
  #pauseReason: "pageBudget" | "assetLookup" | null = null;
  #session: RecoverySession | undefined;
  #page: RecoveryPage | undefined;
  #workOffset = 0;
  #unopenedCount = 0n;
  #queueHead = 0;
  #queued = new Set<string>();

  constructor(
    sync: SyncFields,
    client: AnonymousSyncClient,
    history: ViewingHistory,
    context: RequestContext | undefined,
  ) {
    this.#sync = sync;
    this.#client = client;
    this.#context = context;
    this.#address = sync.keys.address();
    this.#ledger = new NoteLedger(sync.keys, sync.held);
    this.#heldHashes = new Set(sync.held.map(({ outputContext }) => bytesKey(outputContext.hash)));
    this.#originalHistory = history;
    this.#history = history;
  }

  async resume(
    stored: RecoverySession | null,
    ring: Address | undefined,
  ): Promise<RecoverySession | null> {
    const keys = this.#sync.keys.viewingPublicKeys().map(keyHex).sort();
    const owner = bytesToHex(this.#address.toBytes());
    const session = stored ?? this.#newSession({ owner, keys, ring });
    if (
      session.owner !== owner ||
      session.ring !== (ring ?? null) ||
      session.rescan !== this.#sync.fullRescan ||
      session.keys.length !== keys.length ||
      session.keys.some((key) => !keys.includes(key))
    )
      throw new RingError("RING_SCAN_HISTORY_CHANGED");
    this.#session = session;
    this.#history = ViewingHistory.fromJson(session.history);
    this.#unopenedCount = session.unopenedCount;
    for (const hash of session.unopenedSamples)
      this.#unopened.set(hash, hexToBytes(hash) as Bytes32);
    this.#queued = new Set([
      ...session.queue,
      ...session.checked,
      ...(session.active?.nullifiers ?? []),
    ]);
    this.#enqueue(this.#sync.held.filter((note) => !note.spent).map((note) => note.nullifier));
    let remaining = this.#sync.limits.maxPages;
    if (session.targetSlot === null) {
      const first = this.#history.entries.find((entry) =>
        session.keys.includes(keyHex(entry.viewingPublicKey)),
      );
      if (first === undefined) throw new RingError("RING_SCAN_HISTORY_CHANGED");
      const minimumSlot = this.#history.entries.reduce(
        (floor, entry) =>
          entry
            .ringPositions()
            .reduce(
              (highest, position) => maxSlot(highest, position.afterSlot ?? 0n),
              maxSlot(floor, entry.tagPosition ?? 0n),
            ),
        maxSlot(this.#sync.minimumSlot ?? 0n, this.#sync.config?.requireSlot ?? 0n),
      );
      const probe = await this.#client.getEncryptedUtxosByTags(
        { tags: [first.viewingPublicKey.u()], limit: 1, minContextSlot: minimumSlot },
        { poll: this.#sync.config?.poll ?? DEFAULT_INDEXER_POLL_CONFIG, requireSlot: minimumSlot },
        this.#context,
      );
      if (probe.context.slot < minimumSlot) throw new RingError("RING_HISTORY_PAGE_INVALID");
      session.targetSlot = probe.context.slot;
      remaining--;
    }
    if (session.targetSlot < (this.#sync.minimumSlot ?? 0n))
      throw new RingError("RING_SYNC_INCOMPLETE");
    while (remaining > 0) {
      await this.#ledger.assignNullifiers(this.#context);
      this.#enqueue(this.#ledger.takeNullifiers());
      if (session.active === null && this.#queueHead < session.queue.length) {
        session.active = {
          nullifiers: session.queue.slice(this.#queueHead, this.#queueHead + TAGS_PER_REQUEST),
          limit: this.#sync.limits.pageSize,
          token: null,
          workOffset: 0,
        };
        this.#queueHead += session.active.nullifiers.length;
      }
      try {
        if (session.active !== null) {
          remaining--;
          await this.#spendPage(session.active);
          continue;
        }
        const task = session.tasks[session.task];
        if (task === undefined) break;
        remaining--;
        await this.#discoveryPage(task);
      } catch (error) {
        if (
          error instanceof ClientError &&
          error.code === "CLIENT_INDEXER_RESPONSE_TOO_LARGE" &&
          this.#page !== undefined &&
          this.#page.workOffset === 0 &&
          this.#page.limit > 1
        ) {
          this.#page.limit = Math.floor(this.#page.limit / 2);
          continue;
        }
        if (!(error instanceof RecoveryPaused)) throw error;
        this.#pauseReason = "assetLookup";
        break;
      }
    }
    await this.#ledger.assignNullifiers(this.#context);
    this.#enqueue(this.#ledger.takeNullifiers());
    session.queue = session.queue.slice(this.#queueHead);
    this.#queueHead = 0;
    session.history = this.#history.toJson();
    session.unopenedCount = this.#unopenedCount;
    session.unopenedSamples = [...this.#unopened.keys()];
    const complete =
      session.task === session.tasks.length &&
      session.active === null &&
      session.queue.length === 0;
    this.#status = complete ? "Complete" : "Paused";
    this.#pauseReason = complete ? null : (this.#pauseReason ?? "pageBudget");
    if (!complete) this.#history = this.#originalHistory;
    return complete ? null : session;
  }

  #newSession(
    input: Readonly<{
      owner: string;
      keys: string[];
      ring: Address | undefined;
    }>,
  ): RecoverySession {
    const tasks: RecoveryTask[] = [];
    for (const entry of this.#history.entries.filter((candidate) =>
      input.keys.includes(keyHex(candidate.viewingPublicKey)),
    )) {
      const key = keyHex(entry.viewingPublicKey);
      const rings =
        input.ring === undefined ? entry.ringPositions() : [{ ring: input.ring, afterSlot: null }];
      for (const position of rings)
        tasks.push({
          kind: "ring",
          ring: position.ring,
          keys: [key],
          afterSlot: this.#sync.fullRescan ? null : position.afterSlot,
          limit: this.#sync.limits.pageSize,
          token: null,
          workOffset: 0,
        });
    }
    const ringTasks = new Map<string, Extract<RecoveryTask, { kind: "ring" }>>();
    const combined = tasks.filter((task) => {
      if (task.kind !== "ring") return true;
      const key = `${task.ring}:${task.afterSlot}`;
      const previous = ringTasks.get(key);
      if (previous === undefined) {
        ringTasks.set(key, task);
        return true;
      }
      previous.keys.push(...task.keys);
      return false;
    });
    return {
      ...input,
      ring: input.ring ?? null,
      rescan: this.#sync.fullRescan,
      history: this.#history.toJson(),
      targetSlot: null,
      tasks: combined,
      task: 0,
      queue: [],
      checked: [],
      active: null,
      unopenedCount: 0n,
      unopenedSamples: [],
    };
  }

  #enqueue(nullifiers: readonly Bytes32[]): void {
    const session = this.#session;
    if (session === undefined) return;
    for (const nullifier of nullifiers) {
      const key = bytesToHex(nullifier);
      if (unseen(this.#queued, key)) session.queue.push(key);
    }
  }

  #query(page: RecoveryPage, afterSlot?: bigint | null) {
    const target = this.#session?.targetSlot;
    if (target === undefined || target === null) throw new RingError("RING_HISTORY_PAGE_INVALID");
    this.#page = page;
    return {
      limit: page.limit,
      ...(page.token === null ? {} : { paginationToken: page.token }),
      filters: {
        slot: {
          lte: target,
          ...(afterSlot === undefined || afterSlot === null ? {} : { gt: afterSlot }),
        },
      },
      minContextSlot: target,
    };
  }

  #queryConfig(): IndexerRpcConfig {
    const target = this.#session?.targetSlot ?? 0n;
    return { poll: this.#sync.config?.poll ?? DEFAULT_INDEXER_POLL_CONFIG, requireSlot: target };
  }

  #validatePage(
    page: Readonly<{
      context: Readonly<{ slot: bigint }>;
      data: readonly Readonly<{ slot: bigint }>[];
      paginationToken: string | null;
    }>,
    cursor: RecoveryPage,
    afterSlot?: bigint | null,
  ): void {
    const target = this.#session?.targetSlot;
    if (
      target === undefined ||
      target === null ||
      page.context.slot < target ||
      page.data.length > cursor.limit ||
      page.data.some(
        (row) =>
          row.slot > target ||
          (afterSlot !== undefined && afterSlot !== null && row.slot <= afterSlot),
      ) ||
      (page.paginationToken !== null && page.paginationToken === cursor.token)
    )
      throw new RingError("RING_HISTORY_PAGE_INVALID");
    this.#page = cursor;
    this.#workOffset = 0;
  }

  async #spendPage(active: RecoveryPage & { nullifiers: string[] }): Promise<void> {
    const response = await this.#client.getShieldedTransactionsByNullifiers(
      {
        ...this.#query(active),
        nullifiers: active.nullifiers.map((key) => hexToBytes(key) as Bytes32),
      },
      this.#queryConfig(),
      this.#context,
    );
    this.#validatePage(response, active);
    const rows = response.data.flatMap((item) => item.events);
    await keepRingMerges(this.#mergeReads(), rows);
    await this.#ledger.assignNullifiers(this.#context);
    this.#ledger.noteSpends(rows);
    active.token = response.paginationToken;
    active.workOffset = 0;
    const session = this.#session;
    if (active.token === null && session !== undefined) {
      session.checked.push(...active.nullifiers);
      session.active = null;
    }
  }

  async #discoveryPage(task: RecoveryTask): Promise<void> {
    const session = this.#session;
    if (session === undefined) throw new RingError("RING_HISTORY_PAGE_INVALID");
    const response = await this.#client.getShieldedTransactionsByTags(
      { ...this.#query(task, task.afterSlot), tags: [], ringProgramId: task.ring as Address },
      this.#queryConfig(),
      this.#context,
    );
    this.#validatePage(response, task, task.afterSlot);
    const rows = response.data.flatMap((item) => item.events);
    if (rows.some((row) => row.ringProgramId !== task.ring))
      throw new RingError("RING_HISTORY_PAGE_INVALID");
    for (const key of task.keys)
      await this.#tryEverySite(X25519PublicKey.fromBytes(hexToBytes(key)), rows);
    if (this.#workOffset !== task.workOffset) throw new RingError("RING_HISTORY_PAGE_INVALID");
    const fullScan = session.ring !== null;
    if (fullScan) await keepRingMerges(this.#mergeReads(), rows);
    await this.#ledger.assignNullifiers(this.#context);
    this.#ledger.noteSpends(rows);
    if (fullScan) this.#diagnostics(rows);
    task.token = response.paginationToken;
    task.workOffset = 0;
    if (task.token === null) {
      for (const key of task.keys)
        this.#history = this.#history._withRingCheckpoint(
          task.ring as Address,
          X25519PublicKey.fromBytes(hexToBytes(key)),
          session.targetSlot ?? 0n,
        );
      session.task++;
    }
  }

  #diagnostics(rows: readonly IndexedShieldedTransaction[]): void {
    for (const slot of rows.flatMap((row) => row.outputSlots)) {
      if (
        ringBodyOutput(slot.payload) === undefined ||
        this.#ledger.isFound(slot.outputContext.hash)
      )
        continue;
      this.#unopenedCount++;
      if (this.#unopened.size < RECOVERY_DIAGNOSTIC_SAMPLES)
        this.#unopened.set(bytesToHex(slot.outputContext.hash), slot.outputContext.hash);
    }
  }

  pause(): void {
    this.#status = "Paused";
    this.#pauseReason = "assetLookup";
    this.#history = this.#originalHistory;
  }

  /** Entries at one position of one ring share a read, a key added later is its own query. */
  async rings(): Promise<void> {
    const held = this.#sync.keys.viewingPublicKeys();
    for (const read of this.#history._ringReads(held, this.#sync.fullRescan))
      await this.#readRing({ ...read, fullScan: false });
  }

  /** Recovery, every site tried under every held key from the ring's start. */
  async recover(ring: Address): Promise<void> {
    const keys = this.#history.entries
      .filter((held) => this.#holds(held))
      .map((entry) => entry.viewingPublicKey);
    await this.#readRing({ ring, afterSlot: null, keys, fullScan: true });
  }

  /** The spends of every found or held ring note, round after round, open the ring merges among them. */
  merges(): Promise<void> {
    return openMerges(
      this.#mergeReads(),
      (frontier) =>
        this.#paged(frontier, (nullifiers, token, limit) =>
          this.#client
            .getShieldedTransactionsByNullifiers(
              { nullifiers, ...token, limit },
              this.#sync.config,
              this.#context,
            )
            .then((page) => ({ ...page, items: page.data.flatMap((item) => item.events) })),
        ),
      this.#sync.held.flatMap((utxo) => (utxo.spent ? [] : [utxo.nullifier])),
    );
  }

  async result(): Promise<AnonymousScan> {
    const recovery = await this.#ledger.recovery(this.#context);
    for (const note of [...recovery.utxos, ...recovery.pendingOpenings])
      this.#unopened.delete(bytesKey(note.outputContext.hash));
    if (this.#session !== undefined) this.#session.unopenedSamples = [...this.#unopened.keys()];
    const scan: AnonymousScan = Object.freeze({
      ...recovery,
      auditorRecovery: Object.freeze({
        unopenedCount: this.#unopenedCount,
        unopened: Object.freeze(
          [...this.#unopened.entries()]
            .sort(([left], [right]) => (left < right ? -1 : 1))
            .map(([, hash]) => hash),
        ),
      }),
      history: this.#history,
      status: this.#status,
      pauseReason: this.#pauseReason,
      targetSlot: this.#session?.targetSlot ?? null,
      observedSlot: this.#ledger.observedSlot,
    });
    issuedScans.set(scan, { history: this.#originalHistory });
    return scan;
  }

  #holds(entry: ViewingKeyEntry): boolean {
    return this.#sync.keys.viewingPublicKeys().some((key) => key.equals(entry.viewingPublicKey));
  }

  #mergeReads(): MergeReads {
    return {
      keys: this.#sync.keys,
      owner: this.#address,
      assets: this.#sync.assets,
      ledger: this.#ledger,
      held: this.#heldHashes,
      context: this.#context,
    };
  }

  /** Reads to a terminal page, only its `context.slot` becomes a position. */
  async #readRing(
    read: Readonly<{
      ring: Address;
      afterSlot: bigint | null;
      keys: readonly X25519PublicKey[];
      fullScan: boolean;
    }>,
  ): Promise<void> {
    const { ring, afterSlot, keys, fullScan } = read;
    const reach = await readRingPages(
      {
        client: this.#client,
        ringProgramId: ring,
        limits: this.#sync.limits,
        ...(afterSlot === null ? {} : { afterSlot }),
        ...defined({ config: this.#sync.config }),
      },
      async (rows) => {
        // 1. Each page opens before the next arrives, the scan keeps no ring row past its page.
        for (const viewingPublicKey of keys) await this.#tryEverySite(viewingPublicKey, rows);
        // 2. A full scan reads every merge in its pages, any other read finds ring merges by nullifier.
        if (fullScan) await keepRingMerges(this.#mergeReads(), rows);
        await this.#ledger.assignNullifiers(this.#context);
        this.#ledger.noteSpends(rows);
        if (fullScan) this.#diagnostics(rows);
      },
      this.#context,
    );
    if (reach.checkpoint === undefined) {
      this.#status = "Paused";
      this.#pauseReason = "pageBudget";
      return;
    }
    const { checkpoint } = reach;
    this.#history = keys.reduce(
      (history, key) => history._withRingCheckpoint(ring, key, checkpoint),
      this.#history,
    );
  }

  /** No tag selects a site, a tag proves no ownership. */
  async #tryEverySite(
    viewingPublicKey: X25519PublicKey,
    rows: readonly IndexedShieldedTransaction[],
  ): Promise<void> {
    const reads: BodyReads = this.#sync;
    await this.#read(
      viewingPublicKey,
      await bodySecrets(this.#sync.keys, viewingPublicKey, rows.flatMap(ringBodies), this.#context),
    );
    const sent = await ownTransactionKeys(this.#sync.keys, viewingPublicKey, rows, this.#context);
    try {
      for (const [tx, slot, txViewingKey] of sent) {
        const read = new BodyRead(reads);
        read.sentPayments(tx, slot, txViewingKey);
        this.#ledger.absorb(viewingPublicKey, read);
      }
    } finally {
      for (const [, , txViewingKey] of sent) txViewingKey.destroy();
    }
  }

  async #read(
    viewingPublicKey: X25519PublicKey,
    opened: readonly (readonly [Body, Bytes32])[],
  ): Promise<void> {
    const reads: BodyReads = this.#sync;
    await readBodies(reads, opened, async (open) => {
      const step = this.#workOffset++;
      if (this.#page !== undefined && step < this.#page.workOffset) return;
      this.#ledger.absorb(viewingPublicKey, open());
      if (this.#page !== undefined) this.#page.workOffset = step + 1;
    });
  }

  /** Every page of every chunk, a null token ends a chunk's read. */
  async #paged<T>(
    queried: readonly Bytes32[],
    fetch: (
      chunk: Bytes32[],
      token: Readonly<{ paginationToken?: string }>,
      limit: number,
    ) => Promise<Readonly<{ items: readonly T[]; paginationToken: string | null }>>,
  ): Promise<readonly T[]> {
    const items: T[] = [];
    for (let start = 0; start < queried.length; start += TAGS_PER_REQUEST) {
      const chunk = queried.slice(start, start + TAGS_PER_REQUEST);
      const followed = new Set<string>();
      let token: string | undefined;
      for (;;) {
        const asked = token === undefined ? {} : { paginationToken: token };
        const page = await this.#pageLimit.read((limit) => fetch(chunk, asked, limit));
        items.push(...page.items);
        token = page.paginationToken ?? undefined;
        if (token === undefined) break;
        if (followed.has(token))
          throw new ClientError("CLIENT_INVALID_RPC_RESPONSE", {
            details: { path: "$.paginationToken" },
          });
        followed.add(token);
      }
    }
    return items;
  }
}

function keyHex(key: X25519PublicKey): string {
  return bytesToHex(key.toBytes());
}
