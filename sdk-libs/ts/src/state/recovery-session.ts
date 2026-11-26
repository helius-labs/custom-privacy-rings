import { hexToBytes } from "@noble/hashes/utils.js";
import { address } from "@solana/kit";
import { PAGE_LIMIT, SHIELDED_ADDRESS_LENGTH } from "@heliuslabs/spr/internal";
import { ShieldedAddress } from "@heliuslabs/spr/keypair";
import type { Wallet, WalletUtxo } from "@heliuslabs/spr/transaction";
import { RingError } from "../protocol/error.js";
import { wirePrimitives } from "../protocol/wire-primitives.js";
import { ViewingHistory } from "./viewing-history.js";

const SESSION_FIELDS = new Set([
  "owner",
  "keys",
  "mode",
  "rescan",
  "senderFastPath",
  "ring",
  "history",
  "targetSlot",
  "tasks",
  "task",
  "queue",
  "checked",
  "active",
  "unopenedCount",
  "unopenedSamples",
]);
const ACTIVE_FIELDS = new Set(["limit", "token", "workOffset", "nullifiers"]);
const TASK_FIELDS = new Set(["kind", "limit", "token", "workOffset", "ring", "keys", "afterSlot"]);

export const RECOVERY_DIAGNOSTIC_SAMPLES = 64;

export interface RecoveryCoverage {
  readonly status: "Paused" | "Complete";
  readonly targetSlot: bigint | null;
  readonly checkedNullifiers: ReadonlySet<string>;
}

export interface RecoveryWallet extends Wallet {
  recoveryCoverage(): RecoveryCoverage;
}

export interface RecoveryPage {
  limit: number;
  token: string | null;
  workOffset: number;
}

export interface RecoveryRing extends RecoveryPage {
  kind: "ring";
  ring: string;
  keys: string[];
  afterSlot: bigint | null;
}

export type RecoveryTask = RecoveryRing;

export interface RecoverySession {
  owner: string;
  keys: string[];
  mode: "tagHints" | "everySite";
  rescan: boolean;
  senderFastPath: boolean;
  ring: string | null;
  history: string;
  targetSlot: bigint | null;
  tasks: RecoveryTask[];
  task: number;
  queue: string[];
  checked: string[];
  active: (RecoveryPage & { nullifiers: string[] }) | null;
  unopenedCount: bigint;
  unopenedSamples: string[];
}

export function recoverySessionJson(session: RecoverySession | null): string {
  return JSON.stringify(session, (_, value: unknown) =>
    typeof value === "bigint" ? value.toString() : value,
  );
}

export function restoreRecoverySession(value: unknown): RecoverySession | null {
  if (value === null || value === undefined) return null;
  const stored = record(value, SESSION_FIELDS);
  const mode = stored.mode;
  if (mode !== "tagHints" && mode !== "everySite") throw invalid();
  if (typeof stored.rescan !== "boolean" || typeof stored.senderFastPath !== "boolean")
    throw invalid();
  const history = string(stored.history);
  ViewingHistory.fromJson(history);
  const keys = strings(stored.keys).map(key);
  const tasks = list(stored.tasks).map(task);
  const cursor = integer(stored.task);
  if (cursor > tasks.length || keys.length === 0 || new Set(keys).size !== keys.length)
    throw invalid();
  const active = stored.active === null ? null : record(stored.active, ACTIVE_FIELDS);
  const samples = strings(stored.unopenedSamples).map(hash);
  if (
    samples.length > RECOVERY_DIAGNOSTIC_SAMPLES ||
    u64(stored.unopenedCount) < BigInt(samples.length)
  )
    throw invalid();
  const session: RecoverySession = {
    owner: ownerHex(stored.owner),
    keys,
    mode,
    rescan: stored.rescan === true,
    senderFastPath: stored.senderFastPath === true,
    ring: stored.ring === null ? null : address(string(stored.ring)),
    history,
    targetSlot: optionalSlot(stored.targetSlot),
    tasks,
    task: cursor,
    queue: strings(stored.queue).map(hash),
    checked: strings(stored.checked).map(hash),
    active:
      active === null
        ? null
        : {
            ...page(active),
            nullifiers: strings(active.nullifiers).map(hash),
          },
    unopenedCount: u64(stored.unopenedCount),
    unopenedSamples: samples,
  };
  if (
    session.active !== null &&
    (session.active.nullifiers.length === 0 ||
      session.active.nullifiers.length > Number(PAGE_LIMIT) ||
      session.active.workOffset !== 0)
  )
    throw invalid();
  const queued = [...session.queue, ...session.checked, ...(session.active?.nullifiers ?? [])];
  if (new Set(queued).size !== queued.length) throw invalid();
  if (
    session.targetSlot === null &&
    (session.task !== 0 || session.active !== null || session.checked.length !== 0)
  )
    throw invalid();
  for (const item of tasks) {
    if (
      item.keys.length === 0 ||
      new Set(item.keys).size !== item.keys.length ||
      (session.mode === "everySite" && item.ring !== session.ring) ||
      item.keys.some((held) => !keys.includes(held))
    )
      throw invalid();
  }
  return session;
}

export function recoveryAllows(
  coverage: RecoveryCoverage,
  note: WalletUtxo,
  nullifier: string,
): boolean {
  return (
    !note.spent && (coverage.status === "Complete" || coverage.checkedNullifiers.has(nullifier))
  );
}

function task(value: unknown): RecoveryTask {
  const stored = record(value, TASK_FIELDS);
  if (stored.kind !== "ring") throw invalid();
  return {
    ...page(stored),
    kind: "ring",
    ring: address(string(stored.ring)),
    keys: strings(stored.keys).map(key),
    afterSlot: optionalSlot(stored.afterSlot),
  };
}

function page(stored: Record<string, unknown>): RecoveryPage {
  const limit = integer(stored.limit);
  if (limit < 1 || limit > Number(PAGE_LIMIT)) throw invalid();
  return {
    limit,
    token: stored.token === null ? null : string(stored.token),
    workOffset: integer(stored.workOffset),
  };
}

function optionalSlot(value: unknown): bigint | null {
  return value === null ? null : u64(value);
}

const { text, decimalU64 } = wirePrimitives(() => {
  throw invalid();
});

function u64(value: unknown): bigint {
  return decimalU64(value, "u64");
}

/** `parseExact` hands every integer of a wallet document over as a bigint. */
function integer(value: unknown): number {
  if (typeof value !== "bigint" || value < 0n || value > BigInt(Number.MAX_SAFE_INTEGER))
    throw invalid();
  return Number(value);
}

function strings(value: unknown): string[] {
  return list(value).map(string);
}
function list(value: unknown): unknown[] {
  if (!Array.isArray(value)) throw invalid();
  return value;
}
function string(value: unknown): string {
  return text(value, "text");
}
function hash(value: unknown): string {
  const parsed = string(value);
  if (!/^[0-9a-f]{64}$/.test(parsed)) throw invalid();
  return parsed;
}
function key(value: unknown): string {
  const parsed = string(value);
  if (!/^[0-9a-f]{66}$/.test(parsed)) throw invalid();
  return parsed;
}
function ownerHex(value: unknown): string {
  const parsed = string(value);
  if (parsed.length !== SHIELDED_ADDRESS_LENGTH * 2 || !/^[0-9a-f]+$/.test(parsed)) throw invalid();
  try {
    ShieldedAddress.fromBytes(hexToBytes(parsed));
  } catch {
    throw invalid();
  }
  return parsed;
}
function record(value: unknown, allowed: ReadonlySet<string>): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) throw invalid();
  const object = value as Record<string, unknown>;
  if (Reflect.ownKeys(object).some((key) => typeof key !== "string" || !allowed.has(key)))
    throw invalid();
  return object;
}
function invalid(): RingError {
  return new RingError("RING_WALLET_DOCUMENT", { details: { reason: "invalid recovery session" } });
}
