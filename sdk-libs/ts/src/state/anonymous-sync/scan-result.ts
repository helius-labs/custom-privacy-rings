import type { IndexerReader, IndexerRpcConfig } from "@heliuslabs/spr/client";
import type { Address, Bytes32 } from "@heliuslabs/spr/interface";
import type { X25519PublicKey, ShieldedAddress, ShieldedPublicKey } from "@heliuslabs/spr/keypair";
import type {
  AssetRegistry,
  IndexedShieldedTransaction,
  ShieldedKeys,
  WalletUtxo,
} from "@heliuslabs/spr/transaction";
import type { ReplyClaim } from "../../protocol/codec/anonymous-delivery.js";
import type { ScanLimits } from "../../rpc/history.js";
import type { AnonymousWallet } from "../anonymous-wallet.js";
import type { RecoverySession } from "../recovery-session.js";
import type { RingKeys } from "../ring-keys.js";
import { RingError } from "../../protocol/error.js";
import type { ViewingHistory } from "../viewing-history.js";

/** How a scan found a note. */
export type AnonymousStream =
  | Readonly<{ kind: "merge"; firstNullifier: Bytes32 }>
  /** Tried with no use of its tag. */
  | Readonly<{ kind: "fullScan" }>;

export interface AnonymousRingUtxo {
  readonly utxoHash: Bytes32;
  readonly viewingPublicKey: X25519PublicKey;
  /** The registered key or `ZERO_NULLIFIER_PK`, whichever rebuilt. */
  readonly nullifierPublicKey: Bytes32;
  readonly stream: AnonymousStream;
}

export type SentCheck =
  /** A held recipient address rebuilt the published hash under `mint`. */
  | Readonly<{ kind: "verified"; mint: Address }>
  /** No held address names the viewing key and rebuilds the hash. */
  | Readonly<{ kind: "unverified" }>;

/** One payment of a transfer the wallet sent, its key read back from the auditor stream. */
export interface SentPayment {
  readonly txSignature: IndexedShieldedTransaction["txSignature"];
  readonly utxoHash: Bytes32;
  readonly position: number;
  readonly recipientViewingPublicKey: X25519PublicKey;
  /** The held address that rebuilt the hash, `undefined` under an unverified check. */
  readonly ownerPublicKey: ShieldedPublicKey | undefined;
  readonly amount: bigint;
  readonly check: SentCheck;
}

/** A decrypted reply, an unauthenticated claim about who paid, never a verified sender. */
export interface SenderClaim {
  readonly utxoHash: Bytes32;
  readonly claim: ReplyClaim;
}

export interface AnonymousRecovery {
  /** The notes found, spent once a fetched transaction spends them. */
  readonly utxos: readonly WalletUtxo[];
  readonly spentNullifiers: readonly Bytes32[];
  readonly found: readonly AnonymousRingUtxo[];
  /** The replies on found notes, a frame without one adds none. */
  readonly senderClaims: readonly SenderClaim[];
  /** The wallet's payments in the transfers it sent, a body that fails to open left out. */
  readonly sent: readonly SentPayment[];
  /** Notes under a mint the registry lacks, stored and unspendable until it registers, no rescan needed. */
  readonly pendingOpenings: readonly WalletUtxo[];
  /** Rebuilt mints missing from the asset registry. */
  readonly unknownMints: readonly Address[];
  /** The output hash of each note in `pendingOpenings`. */
  readonly unmatched: readonly Bytes32[];
}

/** Bodies no held key rebuilt, an owner's note under a bad body opens only through an auditor read. */
export interface AuditorRecovery {
  /** Commitments, foreign notes and dummies among them. */
  readonly unopened: readonly Bytes32[];
  readonly unopenedCount: bigint;
}

export interface AnonymousScan extends AnonymousRecovery {
  readonly status: "Paused" | "Complete";
  readonly pauseReason: "pageBudget" | "assetLookup" | null;
  readonly targetSlot: bigint | null;
  readonly observedSlot: bigint;
  /** Filled by a full scan alone. */
  readonly auditorRecovery: AuditorRecovery;
  /** Counters, watermarks and ring positions as read, `AnonymousWallet.absorb` stores them with the notes. */
  readonly history: ViewingHistory;
}

/** A scan a sync issued and no absorb took, no copy carries a position past its notes. */
interface IssuedScan {
  readonly history: ViewingHistory;
  readonly wallet?: AnonymousWallet;
  readonly revision?: bigint;
  readonly coreRevision?: number;
  readonly session?: RecoverySession | null;
}
export const issuedScans = new WeakMap<AnonymousScan, IssuedScan>();

/** @internal `AnonymousWallet.absorb` alone. */
export function consumeScan(
  scan: AnonymousScan,
  history: ViewingHistory,
  wallet: AnonymousWallet,
): RecoverySession | null | undefined {
  const original = issuedScans.get(scan);
  if (original === undefined) throw new RingError("RING_SCAN_CONSUMED");
  if (!history._progressMatches(original.history)) throw new RingError("RING_SCAN_HISTORY_CHANGED");
  if (original.wallet === undefined && scan.status === "Paused")
    throw new RingError("RING_SYNC_INCOMPLETE");
  if (original.wallet !== undefined) {
    const current = wallet._recoveryState();
    if (
      original.wallet !== wallet ||
      current.revision !== original.revision ||
      current.coreRevision !== original.coreRevision
    )
      throw new RingError("RING_SCAN_HISTORY_CHANGED");
  }
  issuedScans.delete(scan);
  return original.session;
}

export type AnonymousSyncClient = Pick<
  IndexerReader,
  | "getShieldedTransactionsByTags"
  | "getEncryptedUtxosByTags"
  | "getShieldedTransactionsByNullifiers"
>;

export type SyncKeys = ShieldedKeys & RingKeys;

/** What opens a body, every secret stays with `keys`. */
export interface BodyReads {
  readonly keys: SyncKeys;
  readonly assets: AssetRegistry;
  readonly recipients: readonly ShieldedAddress[];
}

export interface SyncFields extends BodyReads {
  readonly config: IndexerRpcConfig | undefined;
  readonly minimumSlot?: bigint;
  readonly fullRescan: boolean;
  readonly held: readonly WalletUtxo[];
  readonly limits: ScanLimits;
}

export class RecoveryPaused extends Error {}
