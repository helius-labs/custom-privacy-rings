import type { IndexerRpcConfig } from "@heliuslabs/spr/client";
import { equalBytes } from "@heliuslabs/spr/internal";
import type { Address, Bytes32, RequestContext } from "@heliuslabs/spr/interface";
import type { ShieldedAddress } from "@heliuslabs/spr/keypair";
import type { AssetRegistry, ShieldedKeys, WalletUtxo } from "@heliuslabs/spr/transaction";
import { scanLimits } from "../../rpc/history.js";
import type { AnonymousWallet } from "../anonymous-wallet.js";
import { isZeroKeyNote } from "../exposed-notes.js";
import type { RingKeys } from "../ring-keys.js";
import { RingError } from "../../protocol/error.js";
import { ViewingHistory } from "../viewing-history.js";
import {
  type SyncFields,
  type AnonymousSyncClient,
  type AnonymousScan,
  RecoveryPaused,
  issuedScans,
  type AuditorRecovery,
  type SyncKeys,
} from "./scan-result.js";
import { Scan } from "./scan-session.js";

/** The every-site read of each ring of the history, each body tried under every held key with no use of its tag. */
export class AnonymousSync {
  readonly #fields: SyncFields;

  private constructor(fields: SyncFields) {
    this.#fields = fields;
    Object.freeze(this);
  }

  static create(keys: ShieldedKeys & RingKeys, assets: AssetRegistry): AnonymousSync {
    return new AnonymousSync({
      keys,
      assets,
      config: undefined,
      fullRescan: false,
      held: [],
      recipients: [],
      limits: scanLimits({}),
    });
  }

  withIndexerConfig(config: IndexerRpcConfig): AnonymousSync {
    return new AnonymousSync({ ...this.#fields, config });
  }

  /** Every stream from index 0 and every ring from its start, for a wallet that keeps no notes between syncs. */
  withFullRescan(): AnonymousSync {
    return new AnonymousSync({ ...this.#fields, fullRescan: true });
  }

  /** Include pending openings to retain their later spends. */
  withHeldNotes(held: readonly WalletUtxo[]): AnonymousSync {
    return new AnonymousSync({
      ...this.#fields,
      held: Object.freeze([...held]),
    });
  }

  /** The recipients' addresses the wallet holds, a sent payment verifies only against one. */
  withRecipientAddresses(recipients: readonly ShieldedAddress[]): AnonymousSync {
    return new AnonymousSync({
      ...this.#fields,
      recipients: Object.freeze([...recipients]),
    });
  }

  withScanLimits(input: Readonly<{ pageSize?: number; maxPages?: number }>): AnonymousSync {
    return new AnonymousSync({ ...this.#fields, limits: scanLimits(input) });
  }

  /** An entry whose key the holder lacks stays as stored, a held key without an entry is refused until the history adds it. */
  async fetch(
    client: AnonymousSyncClient,
    history: ViewingHistory,
    context?: RequestContext,
  ): Promise<AnonymousScan> {
    const scan = this.#scan(client, history, context);
    try {
      await scan.rings();
      await scan.merges();
    } catch (error) {
      if (!(error instanceof RecoveryPaused)) throw error;
      scan.pause();
    }
    return scan.result();
  }

  async fetchWallet(
    client: AnonymousSyncClient,
    wallet: AnonymousWallet,
    context?: RequestContext,
  ): Promise<AnonymousScan> {
    return this.#fetchWallet(client, wallet, undefined, context);
  }

  async fullScanWallet(
    client: AnonymousSyncClient,
    wallet: AnonymousWallet,
    ring: Address,
    context?: RequestContext,
  ): Promise<AnonymousScan> {
    return this.#fetchWallet(client, wallet, ring, context);
  }

  async #fetchWallet(
    client: AnonymousSyncClient,
    wallet: AnonymousWallet,
    ring: Address | undefined,
    context: RequestContext | undefined,
  ): Promise<AnonymousScan> {
    const original = wallet._recoveryState();
    const history = wallet.history();
    const fields = {
      ...this.#fields,
      minimumSlot: wallet._recoveryMinimumSlot(),
      held: [...wallet.notes(), ...wallet.pendingOpenings()],
    };
    if (!equalBytes(fields.keys.address().toBytes(), wallet.wallet.identity.toBytes()))
      throw new RingError("RING_SCAN_HISTORY_CHANGED");
    if (fields.keys.viewingPublicKeys().some((key) => history.entry(key) === undefined))
      throw new RingError("RING_VIEWING_HISTORY_INVALID", {
        details: { reason: "a held viewing key has no entry" },
      });
    const scan = new Scan(fields, client, history, context);
    const session = await scan.resume(original.session, ring);
    const result = await scan.result();
    issuedScans.set(result, {
      history,
      wallet,
      revision: original.revision,
      coreRevision: original.coreRevision,
      session,
    });
    return result;
  }

  /** Tries every slot of `ring` from its start, spends resolve from the same pages. */
  async fullScan(
    client: AnonymousSyncClient,
    history: ViewingHistory,
    ring: Address,
    context?: RequestContext,
  ): Promise<AnonymousScan> {
    const scan = this.#scan(client, history, context);
    try {
      await scan.recover(ring);
      await scan.merges();
    } catch (error) {
      if (!(error instanceof RecoveryPaused)) throw error;
      scan.pause();
    }
    return scan.result();
  }

  #scan(
    client: AnonymousSyncClient,
    history: ViewingHistory,
    context: RequestContext | undefined,
  ): Scan {
    if (this.#fields.keys.viewingPublicKeys().some((key) => history.entry(key) === undefined))
      throw new RingError("RING_VIEWING_HISTORY_INVALID", {
        details: { reason: "a held viewing key has no entry" },
      });
    return new Scan(this.#fields, client, history, context);
  }
}

export interface RingFullScan {
  readonly status: "Paused" | "Complete";
  readonly pauseReason: "pageBudget" | "assetLookup" | null;
  readonly targetSlot: bigint | null;
  /** Under the wallet's own nullifier key. */
  readonly utxos: readonly WalletUtxo[];
  /** Unspent zero-key notes in publication order. */
  readonly exposedIfSpent: readonly WalletUtxo[];
  /** The nullifiers of the spent zero-key notes. */
  readonly spentExposed: readonly Bytes32[];
  readonly pending: readonly WalletUtxo[];
  readonly auditorRecovery: AuditorRecovery;
}

/** A wrong tag hides nothing here, every held key tries every site of `ring`. */
export async function ringFullScan(
  input: Readonly<{
    keys: SyncKeys;
    assets: AssetRegistry;
    client: AnonymousSyncClient;
    ring: Address;
    config?: IndexerRpcConfig;
  }>,
  context?: RequestContext,
): Promise<RingFullScan> {
  const [current, ...retired] = [...input.keys.viewingPublicKeys()].reverse();
  if (current === undefined)
    throw new RingError("RING_VIEWING_HISTORY_INVALID", {
      details: { reason: "no held viewing key" },
    });
  const history = retired
    .reduce(
      (held, key, index) => held.withRotatedKey(key, BigInt(index + 1)),
      ViewingHistory.create(current, 0n),
    )
    .withRing(input.ring);
  const sync = AnonymousSync.create(input.keys, input.assets);
  const scan = await (
    input.config === undefined ? sync : sync.withIndexerConfig(input.config)
  ).fullScan(input.client, history, input.ring, context);
  const zeroKey = scan.utxos.filter(isZeroKeyNote);
  return Object.freeze({
    utxos: Object.freeze(scan.utxos.filter((note) => !isZeroKeyNote(note))),
    exposedIfSpent: Object.freeze(zeroKey.filter((note) => !note.spent)),
    spentExposed: Object.freeze(zeroKey.filter((note) => note.spent).map((note) => note.nullifier)),
    pending: scan.pendingOpenings,
    auditorRecovery: scan.auditorRecovery,
    status: scan.status,
    pauseReason: scan.pauseReason,
    targetSlot: scan.targetSlot,
  });
}
