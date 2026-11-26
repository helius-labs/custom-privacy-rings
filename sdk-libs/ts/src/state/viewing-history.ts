/** Rust `zolana_ring_client::viewing_history`. */
import { bytesToHex, hexToBytes } from "@noble/hashes/utils.js";
import { address as checkedRing, type Address } from "@solana/kit";
import type { RequestContext } from "@heliuslabs/spr";
import { addressBytes, equalBytes, U64_MAX } from "@heliuslabs/spr/internal";
import { X25519PublicKey, type ViewTag } from "@heliuslabs/spr/keypair";

import { RingError } from "../protocol/error.js";
import { exactlyOne } from "./batch.js";
import type { RingKeys, ViewTagRequest, ViewTagStream } from "./ring-keys.js";

/** How far past its counter a stream is read, zolana's `DEFAULT_TAG_WINDOW`. */
export const TAG_WINDOW = 64n;

/** `scanned` never passes `next`. */
export class StreamCounter {
  /** The index the next request or send takes. */
  readonly next: bigint;
  /** Past the highest index a sync found. */
  readonly scanned: bigint;

  private constructor(next: bigint, scanned: bigint) {
    this.next = next;
    this.scanned = scanned;
    Object.freeze(this);
  }

  static readonly EMPTY: StreamCounter = new StreamCounter(0n, 0n);

  /** Refuses a watermark past `next`. */
  static restore(stored: Readonly<{ next: bigint; scanned: bigint }>): StreamCounter {
    if (stored.scanned > stored.next) throw invalid("a scan watermark passes its next index");
    return new StreamCounter(stored.next, stored.scanned);
  }

  /** @internal Every index taken and `TAG_WINDOW` past them, `zolana/docs/spec.md` First Time Sync Wallet. */
  _listed(): ViewTagRequest["indices"] {
    return Object.freeze({ start: 0n, end: min(this.next + TAG_WINDOW, U64_MAX) });
  }

  /** @internal The indices an earlier sync read through its key's tag position. */
  _readBefore(): ViewTagRequest["indices"] {
    const end = this.scanned === 0n ? 0n : min(this.scanned + TAG_WINDOW, U64_MAX);
    return Object.freeze({ start: 0n, end });
  }

  /** @internal Past the index a request takes, `next` before the call. */
  _minted(): StreamCounter {
    if (this.next === U64_MAX) throw new RingError("RING_COUNTER_OVERFLOW");
    return new StreamCounter(this.next + 1n, this.scanned);
  }

  /** @internal */
  _taken(index: bigint): StreamCounter {
    return this._atLeast(past(index));
  }

  /** @internal */
  _found(index: bigint): StreamCounter {
    const scanned = max(this.scanned, past(index));
    return new StreamCounter(max(this.next, scanned), scanned);
  }

  /** @internal `next` never falls. */
  _atLeast(next: bigint): StreamCounter {
    return new StreamCounter(max(this.next, next), this.scanned);
  }
}

/** The sender tag a wallet's history gives one transfer, a payment carries its own request tag. */
export class TransferTags {
  /** The key the sender tag derives from. */
  readonly viewingPublicKey: X25519PublicKey;
  /** Published once in a frame, on the first slot the wallet's own key opens. */
  readonly sender: ViewTag;
  /** @internal */
  readonly _senderIndex: bigint;

  /** @internal */
  constructor(
    fields: Readonly<{
      viewingPublicKey: X25519PublicKey;
      sender: ViewTag;
      senderIndex: bigint;
    }>,
  ) {
    this.viewingPublicKey = fields.viewingPublicKey;
    this.sender = fields.sender;
    this._senderIndex = fields.senderIndex;
    Object.freeze(this);
  }
}

export class ViewingKeyEntry {
  readonly viewingPublicKey: X25519PublicKey;
  readonly createdAt: bigint;
  readonly tx: StreamCounter;
  readonly request: StreamCounter;
  /** `null` reads the ring from its start. */
  readonly #ringPositions: ReadonlyMap<Address, bigint | null>;
  /** The tag read is complete through it, `null` reads from the start. */
  readonly tagPosition: bigint | null;

  private constructor(fields: EntryFields) {
    this.viewingPublicKey = fields.viewingPublicKey;
    this.createdAt = fields.createdAt;
    this.tx = fields.tx;
    this.request = fields.request;
    this.#ringPositions = new Map(fields.ringPositions);
    this.tagPosition = fields.tagPosition;
    Object.freeze(this);
  }

  static create(viewingPublicKey: X25519PublicKey, createdAt: bigint): ViewingKeyEntry {
    return new ViewingKeyEntry({
      viewingPublicKey,
      createdAt: checkedI64(createdAt, "createdAt"),
      tx: StreamCounter.EMPTY,
      request: StreamCounter.EMPTY,
      ringPositions: new Map(),
      tagPosition: null,
    });
  }

  get txCount(): bigint {
    return this.tx.next;
  }

  get requestCount(): bigint {
    return this.request.next;
  }

  ringPositions(): readonly RingPosition[] {
    return [...this.#ringPositions].map(([ring, afterSlot]) => Object.freeze({ ring, afterSlot }));
  }

  /** `undefined` for a ring the key does not read. */
  ringPosition(ring: Address): bigint | null | undefined {
    return this.#ringPositions.get(ring);
  }

  /** Refuses a ring named twice. */
  static restore(
    fields: Readonly<{
      viewingPublicKey: X25519PublicKey;
      createdAt: bigint;
      tx: StreamCounter;
      request: StreamCounter;
      ringPositions: readonly RingPosition[];
      tagPosition: bigint | null;
    }>,
  ): ViewingKeyEntry {
    const ringPositions = new Map(fields.ringPositions.map((row) => [row.ring, row.afterSlot]));
    if (ringPositions.size !== fields.ringPositions.length)
      throw invalid("ringPositions repeats a ring");
    return new ViewingKeyEntry({
      viewingPublicKey: fields.viewingPublicKey,
      createdAt: checkedI64(fields.createdAt, "createdAt"),
      tx: fields.tx,
      request: fields.request,
      ringPositions,
      tagPosition: fields.tagPosition,
    });
  }

  /** @internal Every stream a sync reads under the key, the bootstrap tag aside. */
  _streams(): readonly (readonly [ViewTagStream, StreamCounter])[] {
    return [
      ["sender", this.tx],
      ["request", this.request],
    ];
  }

  /** @internal A tag found at `index` moves its stream's counter past it. */
  _found(stream: ViewTagStream, index: bigint): ViewingKeyEntry {
    return this.#counted(stream, (counter) => counter._found(index));
  }

  /** @internal A confirmed frame carried the sender tag at `index`. */
  _senderTaken(index: bigint): ViewingKeyEntry {
    return this.#counted("sender", (counter) => counter._taken(index));
  }

  /** @internal */
  _requestMinted(): ViewingKeyEntry {
    return this.#counted("request", (counter) => counter._minted());
  }

  /** @internal */
  _requestsAtLeast(count: bigint): ViewingKeyEntry {
    return this.#counted("request", (counter) => counter._atLeast(count));
  }

  /** @internal */
  _withRingPosition(ring: Address, afterSlot: bigint | null): ViewingKeyEntry {
    return new ViewingKeyEntry({
      ...this.#fields(),
      ringPositions: new Map([...this.#ringPositions, [ring, afterSlot]]),
    });
  }

  /** @internal */
  _withTagPosition(afterSlot: bigint): ViewingKeyEntry {
    return new ViewingKeyEntry({ ...this.#fields(), tagPosition: afterSlot });
  }

  /** @internal A lower `next` would issue a tag again. */
  _progressMatches(stored: ViewingKeyEntry): boolean {
    const counters = COUNTERS.every(
      (stream) =>
        this[stream].next >= stored[stream].next && this[stream].scanned === stored[stream].scanned,
    );
    const keptEvery = stored
      .ringPositions()
      .every(({ ring, afterSlot }) => this.ringPosition(ring) === afterSlot);
    const addedUnread = this.ringPositions().every(
      ({ ring, afterSlot }) => stored.ringPosition(ring) !== undefined || afterSlot === null,
    );
    return counters && keptEvery && addedUnread && this.tagPosition === stored.tagPosition;
  }

  /** @internal `read` is the same key after a scan, a ring added since keeps its unread position. */
  _withScanProgress(read: ViewingKeyEntry): ViewingKeyEntry {
    return new ViewingKeyEntry({
      ...this.#fields(),
      tx: read.tx._atLeast(this.tx.next),
      request: read.request._atLeast(this.request.next),
      ringPositions: new Map([...this.#ringPositions, ...read.#ringPositions]),
    });
  }

  #counted(
    stream: ViewTagStream,
    moved: (counter: StreamCounter) => StreamCounter,
  ): ViewingKeyEntry {
    const counter = stream === "sender" ? "tx" : "request";
    return new ViewingKeyEntry({ ...this.#fields(), [counter]: moved(this[counter]) });
  }

  #fields(): EntryFields {
    return {
      viewingPublicKey: this.viewingPublicKey,
      createdAt: this.createdAt,
      tx: this.tx,
      request: this.request,
      ringPositions: this.#ringPositions,
      tagPosition: this.tagPosition,
    };
  }
}

/** One ring a key reads, `afterSlot` the terminal page's `context.slot` of its last read. */
export type RingPosition = Readonly<{ ring: Address; afterSlot: bigint | null }>;

export type RingRead = RingPosition & Readonly<{ keys: readonly X25519PublicKey[] }>;

/** A tag for one output, `history` advanced past it. */
export interface AdvancedTag<T> {
  readonly tag: T;
  readonly history: ViewingHistory;
}

/** Every retained viewing key, oldest first, the last one current. */
export class ViewingHistory {
  readonly entries: readonly ViewingKeyEntry[];

  private constructor(entries: readonly ViewingKeyEntry[]) {
    this.entries = Object.freeze([...entries]);
    Object.freeze(this);
  }

  /** A wallet whose one viewing key never paid or was paid. */
  static create(viewingPublicKey: X25519PublicKey, createdAt: bigint): ViewingHistory {
    return new ViewingHistory([ViewingKeyEntry.create(viewingPublicKey, createdAt)]);
  }

  /** The rotated key becomes current, its next sync reads every ring of the wallet from its start. */
  withRotatedKey(viewingPublicKey: X25519PublicKey, createdAt: bigint): ViewingHistory {
    if (createdAt <= this.current().createdAt || this.entry(viewingPublicKey) !== undefined)
      throw new RingError("RING_ROTATED_KEY_NOT_NEWER");
    return new ViewingHistory([...this.entries, this.#unreadEntry(viewingPublicKey, createdAt)]);
  }

  /** A key the wallet held before its current one, its next sync reads every ring from its start. */
  withImportedKey(viewingPublicKey: X25519PublicKey, createdAt: bigint): ViewingHistory {
    if (
      createdAt >= this.current().createdAt ||
      this.entries.some((entry) => entry.createdAt === createdAt) ||
      this.entry(viewingPublicKey) !== undefined
    )
      throw new RingError("RING_IMPORTED_KEY_NOT_OLDER");
    return new ViewingHistory(
      [...this.entries, this.#unreadEntry(viewingPublicKey, createdAt)].sort((left, right) =>
        ascending(left.createdAt, right.createdAt),
      ),
    );
  }

  current(): ViewingKeyEntry {
    const current = this.entries.at(-1);
    // Every constructor and `fromJson` holds at least one entry.
    if (current === undefined) throw invalid("holds no viewing key");
    return current;
  }

  entry(viewingPublicKey: X25519PublicKey): ViewingKeyEntry | undefined {
    return this.entries.find((entry) => entry.viewingPublicKey.equals(viewingPublicKey));
  }

  withEntry(index: number, entry: ViewingKeyEntry): ViewingHistory {
    if (this.entries[index] === undefined) throw invalid("names no viewing key at the index");
    return new ViewingHistory(this.entries.map((held, at) => (at === index ? entry : held)));
  }

  /** Every ring any key reads, by address text. */
  rings(): readonly Address[] {
    return [
      ...new Set(this.entries.flatMap((entry) => entry.ringPositions().map(({ ring }) => ring))),
    ].sort();
  }

  withRing(ring: Address, afterSlot: bigint | null = null): ViewingHistory {
    return new ViewingHistory(
      this.entries.map((entry) =>
        entry.ringPosition(ring) === undefined ? entry._withRingPosition(ring, afterSlot) : entry,
      ),
    );
  }

  /** @internal Held keys at one position of one ring share a read, `fromStart` reads every ring from its start. */
  _ringReads(held: readonly X25519PublicKey[], fromStart: boolean): readonly RingRead[] {
    const reads = new Map<string, RingRead>();
    for (const entry of this.entries.filter((candidate) =>
      held.some((key) => key.equals(candidate.viewingPublicKey)),
    ))
      for (const { ring, afterSlot } of entry.ringPositions()) {
        const position = fromStart ? null : afterSlot;
        const key = `${ring}:${String(position)}`;
        const read = reads.get(key) ?? { ring, afterSlot: position, keys: [] };
        reads.set(key, { ...read, keys: [...read.keys, entry.viewingPublicKey] });
      }
    return [...reads.values()];
  }

  /** @internal Only a terminal page's `context.slot` reaches here, a lower one leaves the position in place. */
  _withRingCheckpoint(
    ring: Address,
    viewingPublicKey: X25519PublicKey,
    checkpoint: bigint,
  ): ViewingHistory {
    const index = this.entries.findIndex((entry) =>
      entry.viewingPublicKey.equals(viewingPublicKey),
    );
    const entry = this.entries[index];
    if (entry === undefined) throw invalid("names no entry for the viewing key");
    const stored = entry.ringPosition(ring);
    if (typeof stored === "bigint" && stored >= checkpoint) return this;
    return this.withEntry(index, entry._withRingPosition(ring, checkpoint));
  }

  /** @internal Each named key's tag reads are complete through `afterSlot`. */
  _withTagPosition(keys: readonly X25519PublicKey[], afterSlot: bigint): ViewingHistory {
    return new ViewingHistory(
      this.entries.map((entry) =>
        keys.some((key) => key.equals(entry.viewingPublicKey))
          ? entry._withTagPosition(afterSlot)
          : entry,
      ),
    );
  }

  /** @internal Every stored key's progress unmoved, an added key starts with none. */
  _progressMatches(stored: ViewingHistory): boolean {
    return (
      stored.entries.every((held) => {
        const entry = this.entry(held.viewingPublicKey);
        return entry !== undefined && entry._progressMatches(held);
      }) &&
      this.entries.every(
        (entry) =>
          stored.entry(entry.viewingPublicKey) !== undefined ||
          (entry.tx.scanned === 0n &&
            entry.request.scanned === 0n &&
            entry.ringPositions().every(({ afterSlot }) => afterSlot === null) &&
            entry.tagPosition === null),
      )
    );
  }

  /** @internal A counter, key or ring added during the scan stays. */
  _withScanProgress(scanned: ViewingHistory): ViewingHistory {
    return new ViewingHistory(
      this.entries.map((entry) => {
        const read = scanned.entry(entry.viewingPublicKey);
        return read === undefined ? entry : entry._withScanProgress(read);
      }),
    );
  }

  /** @internal */
  _withFound(
    viewingPublicKey: X25519PublicKey,
    stream: ViewTagStream,
    index: bigint,
  ): ViewingHistory {
    const at = this.entries.findIndex((entry) => entry.viewingPublicKey.equals(viewingPublicKey));
    const entry = this.entries[at];
    return entry === undefined ? this : this.withEntry(at, entry._found(stream, index));
  }

  /** The sender tag of one transfer under the current key, `withPublished` takes its index. */
  async select(keys: RingKeys, context?: RequestContext): Promise<TransferTags> {
    const { viewingPublicKey, tx } = this.current();
    const senderIndex = tx.next;
    const sender = await derivedTag(
      keys,
      { viewingPublicKey, stream: "sender", index: senderIndex },
      context,
    );
    return new TransferTags({ viewingPublicKey, sender, senderIndex });
  }

  /** The history once the transfer of `tags` confirmed, past the sender index when `published` carries its tag. */
  withPublished(tags: TransferTags, published: readonly ViewTag[]): ViewingHistory {
    const at = this.entries.findIndex((entry) =>
      entry.viewingPublicKey.equals(tags.viewingPublicKey),
    );
    const entry = this.entries[at];
    if (entry === undefined || !published.some((tag) => equalBytes(tag, tags.sender))) return this;
    return this.withEntry(at, entry._senderTaken(tags._senderIndex));
  }

  /** The tag of the current key's next payment request. */
  async requestTag(keys: RingKeys, context?: RequestContext): Promise<AdvancedTag<ViewTag>> {
    const { history, tag } = this._takeRequest();
    return Object.freeze({ tag: await tag(keys, context), history });
  }

  /** @internal `history` holds the request index before `tag` asks the holder, concurrent callers never share one. */
  _takeRequest(): Readonly<{
    history: ViewingHistory;
    tag: (keys: RingKeys, context?: RequestContext) => Promise<ViewTag>;
  }> {
    const current = this.current();
    const index = current.request.next;
    return {
      history: this.#withCurrent(current._requestMinted()),
      tag: (keys, context) =>
        derivedTag(
          keys,
          { viewingPublicKey: current.viewingPublicKey, stream: "request", index },
          context,
        ),
    };
  }

  /** At least `count` requests were minted under the current key, a wallet restored from its seed alone states it. */
  withRequestCount(count: bigint): ViewingHistory {
    return this.#withCurrent(this.current()._requestsAtLeast(u64(count, "request count")));
  }

  /** Its positions and counters decide what a sync skips, store it as key material. */
  toJson(): string {
    const counter = (value: StreamCounter): string =>
      `{"next":${value.next},"scanned":${value.scanned}}`;
    const entries = this.entries.map((entry) => {
      const positions = [...entry.ringPositions()]
        .sort((left, right) => byAddressBytes(left.ring, right.ring))
        .map(({ ring, afterSlot }) => `{"ring":"${ring}","afterSlot":${afterSlot ?? "null"}}`);
      return (
        `{"viewingPubkey":"${keyOf(entry.viewingPublicKey)}","createdAt":${entry.createdAt},` +
        `"tx":${counter(entry.tx)},"request":${counter(entry.request)},` +
        `"ringPositions":[${positions.join(",")}],` +
        `"tagPosition":${entry.tagPosition ?? "null"}}`
      );
    });
    return `{"viewingHistory":[${entries.join(",")}]}`;
  }

  static fromJson(text: string): ViewingHistory {
    return ViewingHistory._fromValue(parseExact(text));
  }

  /** @internal Integers already exact bigints. */
  static _fromValue(value: unknown): ViewingHistory {
    const stored = record(value, "history");
    known(stored, "history", ["viewingHistory"]);
    const entries: ViewingKeyEntry[] = [];
    for (const value of list(stored["viewingHistory"], "viewingHistory")) {
      const entry = storedEntry(value);
      const last = entries.at(-1);
      if (
        (last !== undefined && last.createdAt >= entry.createdAt) ||
        entries.some((earlier) => earlier.viewingPublicKey.equals(entry.viewingPublicKey))
      )
        throw invalid("viewing keys repeat or are not oldest first");
      entries.push(entry);
    }
    if (entries.length === 0) throw invalid("holds no viewing key");
    return new ViewingHistory(entries);
  }

  #withCurrent(entry: ViewingKeyEntry): ViewingHistory {
    return this.withEntry(this.entries.length - 1, entry);
  }

  /** Every ring the wallet reads is unread under an added key. */
  #unreadEntry(viewingPublicKey: X25519PublicKey, createdAt: bigint): ViewingKeyEntry {
    return this.rings().reduce(
      (entry, ring) => entry._withRingPosition(ring, null),
      ViewingKeyEntry.create(viewingPublicKey, createdAt),
    );
  }
}

interface EntryFields {
  readonly viewingPublicKey: X25519PublicKey;
  readonly createdAt: bigint;
  readonly tx: StreamCounter;
  readonly request: StreamCounter;
  readonly ringPositions: ReadonlyMap<Address, bigint | null>;
  readonly tagPosition: bigint | null;
}

const COUNTERS = ["tx", "request"] as const;

async function derivedTag(
  keys: RingKeys,
  tag: Readonly<{ viewingPublicKey: X25519PublicKey; stream: ViewTagStream; index: bigint }>,
  context: RequestContext | undefined,
): Promise<ViewTag> {
  const { viewingPublicKey, stream, index } = tag;
  const indices = { start: index, end: past(index) };
  return exactlyOne(
    exactlyOne(await keys.viewTags([{ viewingPublicKey, stream, indices }], context)),
  );
}

const I64_MIN = -(1n << 63n);
const I64_MAX = (1n << 63n) - 1n;

function storedEntry(value: unknown): ViewingKeyEntry {
  const stored = record(value, "entry");
  known(stored, "entry", [
    "viewingPubkey",
    "createdAt",
    "tx",
    "request",
    "ringPositions",
    "tagPosition",
  ]);
  return ViewingKeyEntry.restore({
    viewingPublicKey: pubkey(stored["viewingPubkey"], "viewingPubkey"),
    createdAt: integer(stored["createdAt"], "createdAt"),
    tx: storedCounter(stored["tx"], "tx"),
    request: storedCounter(stored["request"], "request"),
    ringPositions: list(stored["ringPositions"], "ringPositions").map(storedPosition),
    tagPosition: stored["tagPosition"] === null ? null : u64(stored["tagPosition"], "tagPosition"),
  });
}

function storedPosition(value: unknown): RingPosition {
  const stored = record(value, "ringPosition");
  known(stored, "ringPosition", ["ring", "afterSlot"]);
  const afterSlot = stored["afterSlot"];
  return Object.freeze({
    ring: storedRing(stored["ring"]),
    afterSlot: afterSlot === null ? null : u64(afterSlot, "afterSlot"),
  });
}

function storedRing(value: unknown): Address {
  if (typeof value !== "string") throw invalid("ring is not an address");
  try {
    return checkedRing(value);
  } catch {
    throw invalid("ring is not an address");
  }
}

function storedCounter(value: unknown, name: string): StreamCounter {
  const stored = record(value, name);
  known(stored, name, ["next", "scanned"]);
  return StreamCounter.restore({
    next: u64(stored["next"], name),
    scanned: u64(stored["scanned"], name),
  });
}

/** Integer literals parse to exact bigints, a runtime without source access refuses an unsafe one. */
export function parseExact(text: string): unknown {
  if (typeof text !== "string") throw invalid("is not JSON text");
  try {
    return JSON.parse(text, (_key: string, value: unknown, context?: { source?: string }) => {
      if (typeof value !== "number") return value;
      const source = context?.source;
      if (source !== undefined && /^-?[0-9]+$/.test(source)) return BigInt(source);
      return Number.isSafeInteger(value) ? BigInt(value) : value;
    });
  } catch (cause) {
    throw new RingError("RING_VIEWING_HISTORY_INVALID", {
      details: { reason: "does not parse as JSON" },
      cause,
    });
  }
}

function record(value: unknown, name: string): Readonly<Record<string, unknown>> {
  if (typeof value !== "object" || value === null || Array.isArray(value))
    throw invalid(`${name} is not an object`);
  return value as Readonly<Record<string, unknown>>;
}

function list(value: unknown, name: string): readonly unknown[] {
  if (!Array.isArray(value)) throw invalid(`${name} is not a list`);
  return value;
}

function known(value: Readonly<Record<string, unknown>>, name: string, keys: readonly string[]) {
  const missing = keys.some((key) => !(key in value));
  const unknown = Object.keys(value).some((key) => !keys.includes(key));
  if (missing || unknown) throw invalid(`${name} names other fields`);
}

function integer(value: unknown, name: string): bigint {
  if (typeof value !== "bigint") throw invalid(`${name} is not an integer`);
  return value;
}

function u64(value: unknown, name: string): bigint {
  const parsed = integer(value, name);
  if (parsed < 0n || parsed > U64_MAX) throw invalid(`${name} is outside u64`);
  return parsed;
}

function checkedI64(value: bigint, name: string): bigint {
  if (typeof value !== "bigint" || value < I64_MIN || value > I64_MAX)
    throw invalid(`${name} is outside i64`);
  return value;
}

/** An error names the field alone. */
function pubkey(value: unknown, name: string): X25519PublicKey {
  if (typeof value !== "string" || !/^[0-9a-fA-F]{66}$/.test(value))
    throw invalid(`${name} holds a key that is not a tagged X25519 key`);
  try {
    return X25519PublicKey.fromBytes(hexToBytes(value));
  } catch {
    throw invalid(`${name} holds a key that is not a tagged X25519 key`);
  }
}

function keyOf(key: X25519PublicKey): string {
  return bytesToHex(key.toBytes());
}

function invalid(reason: string): RingError {
  return new RingError("RING_VIEWING_HISTORY_INVALID", { details: { reason } });
}

/** `index + 1`, held at `u64::MAX` as Rust `saturating_add`. */
function past(index: bigint): bigint {
  return min(index + 1n, U64_MAX);
}

function max(left: bigint, right: bigint): bigint {
  return left > right ? left : right;
}

function min(left: bigint, right: bigint): bigint {
  return left < right ? left : right;
}

/** Rust orders a `BTreeMap<Address, _>` by the address bytes, base58 text orders another way. */
function byAddressBytes(left: Address, right: Address): number {
  const [leftBytes, rightBytes] = [addressBytes(left), addressBytes(right)];
  const at = leftBytes.findIndex((byte, index) => byte !== rightBytes[index]);
  return at < 0 ? 0 : (leftBytes[at] ?? 0) - (rightBytes[at] ?? 0);
}

function ascending(left: bigint, right: bigint): number {
  return left < right ? -1 : left > right ? 1 : 0;
}
