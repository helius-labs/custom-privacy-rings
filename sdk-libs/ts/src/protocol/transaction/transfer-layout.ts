/**
 * The windows of a `Transfer` or `AuthorityTransfer` account list, Rust `TransferLayout`.
 *
 * SPP reads its list by position and the ring program prepends its own slots. A reader cutting a
 * window at another index would check or sign accounts the program does not read there.
 * `test-vectors/proof/transfer-layout.json` pins the windows.
 */
import { NO_CACHE_WRITES } from "@heliuslabs/spr/interface";
import type { CacheAccess, InterfaceTransfer } from "@heliuslabs/spr/interface";

import { ringRootRegistries, type RingPolicy } from "../codec/ring-codecs.js";
import { RingError } from "../error.js";
import { isDelegateRail, type RingRail, type RingTransactData } from "./transact.js";
import { defined } from "../defined.js";

/** Rust `TRANSACT_PAYER_INDEX`. */
const TRANSACT_PAYER_INDEX = 0;
/** Rust `RING_TRANSACT_RING_CONFIG_INDEX`, the ring auth PDA's place in SPP's list. */
export const RING_TRANSACT_RING_CONFIG_INDEX = 4;
/** Rust `RING_TRANSACT_ACCOUNT_PREFIX_LEN`. */
const RING_TRANSACT_ACCOUNT_PREFIX_LEN = RING_TRANSACT_RING_CONFIG_INDEX + 1;

/** `[start, end)` over the whole list. */
export interface AccountWindow {
  readonly start: number;
  readonly end: number;
}

/** A prefix its builder or a located SPP window fixes, Rust `KnownPrefix`. */
export interface KnownPrefix {
  readonly coSignerPairs: number;
  readonly delegateMint: boolean;
}

/** Rust `SettlementLeg`. */
export interface SettlementLeg {
  readonly transfer: InterfaceTransfer;
  readonly accounts: AccountWindow;
  /** The signer a deposit moves funds from. */
  readonly funder?: number;
}

/** Indices into the whole list, the ring account at index 0. */
export interface TransferLayout {
  readonly registries: AccountWindow;
  /** `[record, signer]` pairs. */
  readonly coSigners: AccountWindow;
  readonly delegate?: number;
  readonly mint?: number;
  /** The first account SPP receives. */
  readonly spp: number;
  readonly payer: number;
  /** The one slot of the ring auth PDA. */
  readonly ringConfig: number;
  readonly trees: AccountWindow;
  readonly nullifiers: AccountWindow;
  readonly owners: AccountWindow;
  readonly settlement: AccountWindow;
  readonly cache: AccountWindow;
}

type LayoutFrame = Pick<
  RingTransactData,
  "rail" | "inputTrees" | "nullifiers" | "interfaceTransfers" | "extensions"
>;

/** Rust `InterfaceTransfer::settlement_account_count`. */
export function settlementAccountCount(transfer: InterfaceTransfer): number {
  return isSolLeg(transfer) ? 2 : 5;
}

/** Rust `InterfaceTransfer::mint_account_position`, a SOL leg names no mint. */
export function mintAccountPosition(transfer: InterfaceTransfer): number | undefined {
  switch (transfer.kind) {
    case "splDeposit":
      return 0;
    case "splWithdrawal":
      return 1;
    case "solDeposit":
    case "solWithdrawal":
      return undefined;
  }
}

/** Rust `funder_account_position`, a withdrawal names no funder. */
function funderAccountPosition(transfer: InterfaceTransfer): number | undefined {
  switch (transfer.kind) {
    case "solDeposit":
      return 1;
    case "splDeposit":
      return 2;
    case "solWithdrawal":
    case "splWithdrawal":
      return undefined;
  }
}

export function isDepositLeg(transfer: InterfaceTransfer): boolean {
  return transfer.kind === "solDeposit" || transfer.kind === "splDeposit";
}

export function isSolLeg(transfer: InterfaceTransfer): boolean {
  return transfer.kind === "solDeposit" || transfer.kind === "solWithdrawal";
}

/** Rust `RingTransactData::cache_accounts`, the read cache then the write cache and its writer. */
export function cacheAccountCount(cache: CacheAccess | undefined): number {
  if (cache === undefined) return 0;
  const [none] = NO_CACHE_WRITES;
  const [first] = cache.writeSlots;
  const writes = first !== undefined && (first.output !== none?.output || first.slot !== none.slot);
  return (cache.readBitmap !== 0n ? 1 : 0) + (writes ? 2 : 0);
}

/** Rust `registry_count`, the registries a transfer on `rail` passes after the ring account. */
export function transferRegistryCount(ring: RingPolicy, rail: RingRail): number {
  return ringRootRegistries(ring, isDelegateRail(rail) ? "delegateTransfer" : "transfer").length;
}

/** Rust `KnownPrefix::ending_at`, a delegate frame takes no pair. */
export function knownPrefixEndingAt(
  frame: Pick<RingTransactData, "rail">,
  registries: number,
  spp: number,
): KnownPrefix {
  const delegate = isDelegateRail(frame.rail);
  const slots = spp - 1 - registries;
  if (delegate && (slots === 1 || slots === 2))
    return { coSignerPairs: 0, delegateMint: slots === 2 };
  if (!delegate && slots >= 0 && slots % 2 === 0)
    return { coSignerPairs: slots / 2, delegateMint: false };
  throw invalid();
}

/** Rust `TransferLayout::new` over `registries` live registries, a list too short for the frame's counts is refused. */
export function transferLayout(
  frame: LayoutFrame,
  registryCount: number,
  accounts: number,
  prefix: KnownPrefix,
): TransferLayout {
  const delegateRail = isDelegateRail(frame.rail);
  const registries = window(1, registryCount);
  const coSigners = window(registries.end, 2 * prefix.coSignerPairs);
  const delegate = delegateRail ? coSigners.end : undefined;
  const mint = delegate !== undefined && prefix.delegateMint ? delegate + 1 : undefined;
  const spp = coSigners.end + (delegate === undefined ? 0 : 1) + (mint === undefined ? 0 : 1);
  const trees = window(
    spp + RING_TRANSACT_ACCOUNT_PREFIX_LEN,
    frame.inputTrees.kind === "one" ? 1 : 2,
  );
  const nullifiers = window(trees.end, frame.nullifiers.length);
  // The cached selector's accounts follow the settlement groups, a run read through them would
  // take a cache as an unsigned owner.
  const cacheStart = accounts - cacheAccountCount(frame.extensions.cache);
  const settlementStart =
    cacheStart -
    frame.interfaceTransfers.reduce((total, leg) => total + settlementAccountCount(leg), 0);
  if (settlementStart < nullifiers.end) throw invalid();
  return Object.freeze({
    registries,
    coSigners,
    ...defined({ delegate, mint }),
    spp,
    payer: spp + TRANSACT_PAYER_INDEX,
    ringConfig: spp + RING_TRANSACT_RING_CONFIG_INDEX,
    trees,
    nullifiers,
    owners: { start: nullifiers.end, end: settlementStart },
    settlement: { start: settlementStart, end: cacheStart },
    cache: { start: cacheStart, end: accounts },
  });
}

/** Rust `TransferLayout::co_signer_pairs`. */
export function coSignerPairs(
  layout: Pick<TransferLayout, "coSigners">,
): readonly Readonly<{ record: number; signer: number }>[] {
  const { start, end } = layout.coSigners;
  return Array.from({ length: (end - start) / 2 }, (_, pair) => ({
    record: start + 2 * pair,
    signer: start + 2 * pair + 1,
  }));
}

/** Rust `TransferLayout::legs`, the groups of the frame's legs in leg order. */
export function settlementLegs(
  layout: Pick<TransferLayout, "settlement">,
  transfers: readonly InterfaceTransfer[],
): readonly SettlementLeg[] {
  let start = layout.settlement.start;
  return transfers.map((transfer) => {
    const accounts = window(start, settlementAccountCount(transfer));
    start = accounts.end;
    const funder = funderAccountPosition(transfer);
    return {
      transfer,
      accounts,
      ...(funder === undefined ? {} : { funder: accounts.start + funder }),
    };
  });
}

function window(start: number, length: number): AccountWindow {
  return { start, end: start + length };
}

function invalid(): RingError {
  return new RingError("RING_SETTLEMENT_ACCOUNTS_INVALID");
}
