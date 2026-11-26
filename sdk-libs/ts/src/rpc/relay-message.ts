/** The message a wallet signs for Ring RPC, checked against the request the wallet made, Rust `relay_message`. */
import {
  type AccountMeta,
  AccountRole,
  type Address,
  address,
  decompileTransactionMessage,
  getBase58Decoder,
  getCompiledTransactionMessageDecoder,
  getCompiledTransactionMessageEncoder,
  type Instruction,
  isSignerRole,
  isWritableRole,
  type TransactionMessageBytes,
  type TransactionPartialSigner,
} from "@solana/kit";

import { getCreateAssociatedTokenAccountInstructionAsync } from "@heliuslabs/spr/instructions";
import {
  SOL_INTERFACE,
  SHIELDED_POOL_CPI_AUTHORITY,
  TRANSACTION_SIZE_LIMIT,
} from "@heliuslabs/spr/interface";
import { getAssociatedTokenAddress } from "@heliuslabs/spr/addresses";
import type {
  Bytes32,
  RequestContext,
  Signature,
  TransactWithdrawal,
} from "@heliuslabs/spr/interface";
import { slotTag } from "../crypto/derivation/ring-stream.js";
import {
  addressBytes,
  encodeBase58,
  equalBytes,
  isCanonicalField,
  isZero,
  LOADED_ACCOUNTS_DATA_SIZE_LIMIT,
  splInterfaceWithBump,
  ZERO_32,
  ZERO_NULLIFIER_PK,
} from "@heliuslabs/spr/internal";
import {
  ShieldedPublicKey,
  type NullifierKey,
  type ShieldedAddress,
  type X25519PublicKey,
} from "@heliuslabs/spr/keypair";
import { ProofInputUtxo, Utxo, SOL_MINT, type WalletUtxo } from "@heliuslabs/spr/transaction";

import { ringComputeUnitLimit } from "../operations/management/admin.js";
import { RingError } from "../protocol/error.js";
import { wirePrimitives } from "../protocol/wire-primitives.js";
import {
  openingTotals,
  type ApprovalOpening,
  type OutputTotalsOpening,
  type PublicTotalsOpening,
} from "../protocol/membership/approval-totals.js";
import type { OwnerDebit } from "../protocol/membership/owner-debits.js";
import { memberOfAsset } from "../protocol/membership/policy.js";
import { ringAuthAddress, ringCoSignerAddress, ringPolicyAddress } from "../protocol/pda/index.js";
import type {
  AuthorityWithdrawal,
  PrepareAuthorityTransferRequest,
  RelayQuote,
} from "../protocol/ring-rpc/ring-rpc-types.js";
import { RingInstructionTag } from "../protocol/transaction/instruction-tag.js";
import {
  isAnonymousFrame,
  RingTransactData,
  ringRailOf,
  type RingRail,
} from "../protocol/transaction/transact.js";
import {
  coSignerPairs,
  isDepositLeg,
  knownPrefixEndingAt,
  mintAccountPosition,
  RING_TRANSACT_RING_CONFIG_INDEX,
  settlementLegs,
  transferLayout,
  transferRegistryCount,
  type AccountWindow,
  type SettlementLeg,
  type TransferLayout,
} from "../protocol/transaction/transfer-layout.js";
import type { RingPolicy } from "../protocol/codec/ring-codecs.js";
import { RecordKind, type OutputRecord } from "../protocol/codec/audit-stream.js";
import { RECIPIENT_KEY_LEN } from "../protocol/codec/ring-disclosure-layout.js";
import {
  canonicalViewingKey,
  openExitRecord,
  openOutputSlot,
  openSealedDisclosure,
  type RingDisclosure,
} from "../crypto/audit/ring-disclosure.js";
import { txKeySlotSecret } from "../crypto/derivation/ring-stream.js";
import { dummyUtxoHash } from "../protocol/membership/utxo-hash.js";
import { x25519FromU } from "../crypto/keys/x25519.js";
import { rawJson } from "../protocol/codec/raw-json.js";
import { N_PUBLIC_SLOTS } from "../protocol/transaction/spp.js";
import {
  MAX_TRANSACTION_ADDRESSES,
  MAX_TRANSACTION_INSTRUCTIONS,
  MAX_TRANSACTION_SIGNATURES,
} from "../protocol/transaction/v1-limits.js";

const base58 = getBase58Decoder();
const VERIFIED = Symbol("verified relay message");

/** Rust `MessageMismatch`, the variant names the shared vectors carry. */
export type RelayMessageMismatch =
  | "Undecodable"
  | "NotV1"
  | "FeePayer"
  | "Budget"
  | "Instructions"
  | "Program"
  | "Data"
  | "Accounts"
  | "Recipients"
  | "TransactionKey"
  | "ApprovalMaskUnaccepted"
  | "MissingApprovalDigest"
  | "FalseApprovalOpening"
  | "ApprovalOpeningLayout"
  | "ZeroApprovalBlinder"
  | "ReusedApprovalBlinder"
  | "ApprovalOverLimit"
  | "UnopenedOutput"
  | "Legs"
  | "SpendsUnnamedNote"
  | "SpendsUnrequestedNote"
  | "Unrequested"
  | "Signer";

/** Rust `AllowedNullifiers`, filled only by `admit`. */
export class AllowedNullifiers {
  readonly #sources = new Set<string>();
  readonly #nullifiers = new Set<string>();

  /** Rust `AllowedNullifiers::admit`, admits nothing of `source` when one opening fails. */
  admit(
    source: Readonly<{ address: ShieldedAddress; nullifierKey: NullifierKey }>,
    notes: readonly ProofInputUtxo[],
  ): this {
    const { address, nullifierKey } = source;
    if (!equalBytes(nullifierKey.publicKey(), address.nullifierPublicKey))
      throw new RingError("RING_NULLIFIER_KEY_MISMATCH");
    const owner = address.signingPublicKey.confidentialViewTag();
    const nullifiers = notes.map((note, index) => {
      // 1. Owner and key are opening fields the rebuilt hash commits, so the match binds the note.
      if (
        !equalBytes(note.utxo.owner.confidentialViewTag(), owner) ||
        !equalBytes(note.nullifierPublicKey, address.nullifierPublicKey)
      )
        throw delegateSourceRefusal("foreignNote", index);
      // 2. Rebuilt, not read, any carried nullifier passes under the shared zero key.
      const hashes = {
        ...(note.dataHash === undefined ? {} : { dataHash: note.dataHash }),
        ...(note.ringDataHash === undefined ? {} : { ringDataHash: note.ringDataHash }),
      };
      const rebuilt = ProofInputUtxo.fromNullifierKey(note.utxo, nullifierKey, hashes, note.treeId);
      if (!equalBytes(rebuilt.nullifier(), note.nullifier()))
        throw delegateSourceRefusal("unboundOpening", index);
      return rebuilt.nullifier();
    });
    this.#sources.add(encodeBase58(address.toBytes()));
    for (const nullifier of nullifiers) this.#nullifiers.add(encodeBase58(nullifier));
    return this;
  }

  /** Rust `AllowedNullifiers::check_sources`. */
  checkSources(requested: readonly ShieldedAddress[]): void {
    const named = new Set(requested.map((address) => encodeBase58(address.toBytes())));
    const same =
      named.size === this.#sources.size && [...named].every((source) => this.#sources.has(source));
    if (!same) throw new RingError("RING_DELEGATE_INVALID", { details: { reason: "sources" } });
  }

  /**
   * The proof names `spent` notes and no note position. An admitted nullifier is a real note's and
   * SPP publishes it at most once, so the published ones the set holds must count every spent note.
   */
  countsSpends(spent: number, published: Iterable<Bytes32>): boolean {
    let allowed = 0;
    for (const nullifier of published) if (this.#nullifiers.has(encodeBase58(nullifier))) allowed++;
    return allowed === spent;
  }
}

function delegateSourceRefusal(reason: "foreignNote" | "unboundOpening", index: number): RingError {
  return new RingError("RING_DELEGATE_INVALID", { details: { reason, index } });
}

interface ExpectedOwner {
  readonly ownerKey: Bytes32;
  readonly nullifierPk: Bytes32;
  readonly viewingU: Bytes32;
}

interface ExpectedPayment {
  readonly kind: typeof RecordKind.ring | typeof RecordKind.exit;
  readonly owner: ExpectedOwner;
  readonly mint: Address;
  readonly amount: bigint;
}

/** Rust `ExpectedDelegate`. */
export interface ExpectedDelegate {
  readonly kind: "delegate";
  readonly ring: Address;
  readonly ringAuth: Address;
  readonly policy: Address;
  readonly feePayer: Address;
  readonly delegate: Address;
  /** Sizes the registry window of the account list. */
  readonly ringPolicy: RingPolicy;
  /** The frame the ring's mode fixes. */
  readonly rail: RingRail;
  readonly auditor: X25519PublicKey;
  readonly txViewingKey: Bytes32;
  /** Change returns to an input's owner. */
  readonly sources: readonly ExpectedOwner[];
  readonly payments: readonly ExpectedPayment[];
  /** Every leg of the frame in frame order. */
  readonly withdrawals: readonly PublicLeg[];
  /** Every note of each requested source, undefined for a mint's permanent delegate. */
  readonly allowed: AllowedNullifiers | undefined;
  readonly outputTreeId: number;
}

/** Holds no transaction key, the signer reads every published slot before it signs. */
export interface RelayTransferTerms {
  readonly ring: Address;
  readonly feePayer: Address;
  readonly signer: Address;
  /** One per published slot in slot order, each rebuilds its slot's commitment. */
  readonly slots: readonly SlotOpening[];
  /** The signer's notes the transfer spends. */
  readonly nullifiers: readonly Bytes32[];
  /** Every leg of the frame in frame order. */
  readonly legs: readonly PublicLeg[];
  /** The token accounts the fee payer creates ahead of the ring instruction. */
  readonly setup: readonly TokenAccountCreate[];
}

/** What one public funder agrees to, its own deposit legs alone and no slot or note. */
export interface RelayFunderTerms {
  readonly ring: Address;
  readonly feePayer: Address;
  readonly signer: Address;
  /** The legs the key funds in frame order. */
  readonly legs: readonly PublicLeg[];
  /** The token accounts the fee payer creates ahead of the ring instruction. */
  readonly setup: readonly TokenAccountCreate[];
}

/** One interface transfer leg a key agrees to, Rust `PublicLeg`. */
export interface PublicLeg {
  readonly direction: "deposit" | "withdrawal";
  /** Null on SOL. */
  readonly mint: Address | null;
  readonly amount: bigint;
  /** The leg's whole settlement group in frame order. */
  readonly accounts: readonly Address[];
}

/** A zero owner opens a dummy under its blinding alone. */
export interface SlotOpening {
  readonly slot: number;
  readonly owner: Address;
  readonly nullifierPk: Bytes32;
  readonly mint: Address;
  readonly amount: bigint;
  readonly blinding: Bytes32;
  readonly treeId: number;
  readonly dataHash: Bytes32;
  readonly ringDataHash: Bytes32;
}

export interface TokenAccountCreate {
  readonly owner: Address;
  readonly mint: Address;
  readonly tokenProgram: Address;
}

export interface ExpectedPublicOwner {
  readonly kind: "publicOwner";
  readonly terms: RelayTransferTerms;
  readonly policy: Address;
  readonly ringPolicy: RingPolicy;
  /** Finds SPP's account list inside the ring instruction's. */
  readonly ringAuth: Address;
  readonly setup: readonly Instruction[];
  /** From the ring account, a frame read under the wrong rail proves nothing. */
  readonly rail: RingRail;
  readonly knownNullifiers: readonly Bytes32[];
}

/** One public funder reading the frame's deposit legs against its terms, Rust `ExpectedFunder`. */
export interface ExpectedFunder {
  readonly kind: "funder";
  readonly terms: RelayFunderTerms;
  readonly policy: Address;
  readonly ringPolicy: RingPolicy;
  readonly ringAuth: Address;
  readonly setup: readonly Instruction[];
  readonly rail: RingRail;
}

/** The sealing wallet hands each owner its terms and signs nothing itself. */
export interface ExpectedSealed {
  readonly kind: "sealed";
  readonly terms: readonly RelayTransferTerms[];
  /** One per key funding a deposit leg, the payer and an owner of the run left out. */
  readonly funders?: readonly RelayFunderTerms[];
  /** The opening of the frame's digest, handed to an outside co-signer. */
  readonly approval?: ApprovalOpening;
  readonly chargedOutflows: readonly OwnerDebit[];
}

/** An outside co-signer reads the proven bits or the totals, never a slot, Rust `CoSignerRule`. */
export type CoSignerRule =
  /** The proven mask alone, no opening and no digest. */
  | Readonly<{ kind: "threshold"; accepts: number }>
  | Readonly<{ kind: "exact"; approval: ExactApproval }>;

export interface ExactApproval {
  readonly opening: ApprovalOpening;
  /** An asset the totals name with no limit here is refused. */
  readonly limits: readonly ApprovalLimit[];
  /** The blinders of its earlier attempts, the only record of reuse the SDK has. */
  readonly seenBlinders: readonly Bytes32[];
}

/**
 * `peak` bounds one owner's outflow, `outflow` the sum over owners, `gross` each public row's
 * withdrawals with no deposit subtracted.
 */
export interface ApprovalLimit {
  readonly mint: Address;
  readonly peak: bigint;
  readonly outflow: bigint;
  readonly gross: bigint;
}

/** A co-signer the frame names, what it signs is the rule it holds, Rust `ExpectedCoSigner`. */
export interface ExpectedCoSigner {
  readonly kind: "coSigner";
  readonly ring: Address;
  readonly ringAuth: Address;
  readonly policy: Address;
  readonly ringPolicy: RingPolicy;
  readonly feePayer: Address;
  readonly signer: Address;
  /** The signer's co-signer record, first of its pair. */
  readonly record: Address;
  /** From the ring account, a frame read under the wrong rail proves nothing. */
  readonly rail: RingRail;
  readonly setup: readonly Instruction[];
  readonly rule: CoSignerRule;
}

export type ExpectedRelayMessage =
  | ExpectedDelegate
  | ExpectedSealed
  | ExpectedPublicOwner
  | ExpectedFunder
  | ExpectedCoSigner;

type ExpectedFrame = ExpectedDelegate | ExpectedPublicOwner | ExpectedFunder | ExpectedCoSigner;

/** Rust `VerifiedMessage`, only a check builds one, over the private copy that check read. */
export class VerifiedRelayMessage {
  readonly signer: Address;
  readonly #bytes: Uint8Array;

  /** @internal */
  constructor(token: typeof VERIFIED, signer: Address, bytes: Uint8Array) {
    if (token !== VERIFIED) throw mismatch("Unrequested");
    this.signer = signer;
    this.#bytes = bytes;
    Object.freeze(this);
  }

  async sign(signer: TransactionPartialSigner, context?: RequestContext): Promise<Signature> {
    if (signer.address !== this.signer) throw mismatch("Signer");
    const transaction = {
      messageBytes: this.#bytes as unknown as TransactionMessageBytes,
      // A kit keypair signs only an address the transaction lists.
      signatures: { [signer.address]: null },
    } as unknown as Parameters<TransactionPartialSigner["signTransactions"]>[0][number];
    const [signatures] = await signer.signTransactions(
      [transaction],
      context?.signal === undefined ? undefined : { abortSignal: context.signal },
    );
    const signature = signatures?.[signer.address];
    if (!(signature instanceof Uint8Array) || signature.length !== 64) throw mismatch("Signer");
    return base58.decode(signature) as Signature;
  }
}

/** Rust `DelegateRequester`, the ring's delegate with its openings or the permanent delegate of the one mint of every leg. */
export type DelegateRequester =
  | { readonly allowed: AllowedNullifiers; readonly mint?: never }
  | { readonly mint: Address; readonly allowed?: never };

/** Rust `RequestedAuthorityTransfer`. */
export type RequestedAuthorityTransfer = DelegateRequester & {
  readonly request: PrepareAuthorityTransferRequest;
  readonly quote: RelayQuote;
  /** The decoded ring account. */
  readonly config: RingPolicy;
  /** The tree SPP appends the outputs to, each output's commitment rebuilds in it. */
  readonly outputTreeId: number;
};

/** Undefined for a mint's permanent delegate, no source opens to it. */
export function openedSources({
  request,
  config,
  allowed,
  mint,
}: RequestedAuthorityTransfer): AllowedNullifiers | undefined {
  const { transfer } = request;
  const refused = (reason: "missing" | "unauthorized") =>
    new RingError("RING_DELEGATE_INVALID", { details: { reason } });
  if (allowed === undefined) {
    const legs = [...transfer.payments, ...transfer.exits, ...transfer.withdrawals];
    if (legs.length === 0 || legs.some((leg) => leg.mint !== mint)) throw refused("unauthorized");
    return undefined;
  }
  if (config.delegate !== request.delegate)
    throw refused(config.delegate === undefined ? "missing" : "unauthorized");
  allowed.checkSources(transfer.sources);
  return allowed;
}

export async function expectedDelegate(
  requested: RequestedAuthorityTransfer,
): Promise<ExpectedDelegate> {
  const { request, quote, config, outputTreeId } = requested;
  const { transfer } = request;
  const allowed = openedSources(requested);
  if (!Number.isInteger(outputTreeId) || outputTreeId < 0 || outputTreeId > 0xffff)
    throw new RingError("RING_DELEGATE_INVALID", { details: { reason: "outputTreeId" } });
  return Object.freeze({
    kind: "delegate",
    ring: request.ring,
    ringAuth: await ringAuthAddress(request.ring),
    policy: await ringPolicyAddress(request.ring),
    feePayer: quote.feePayer,
    delegate: request.delegate,
    ringPolicy: config,
    rail: ringRailOf(RingInstructionTag.authorityTransfer, config.mode),
    auditor: config.auditorPublicKey,
    txViewingKey: request.txViewingKey,
    sources: Object.freeze(transfer.sources.map(expectedOwner)),
    payments: Object.freeze(
      [
        ...transfer.payments.map((payment) => ({ kind: RecordKind.ring, payment })),
        ...transfer.exits.map((payment) => ({ kind: RecordKind.exit, payment })),
      ].map(({ kind, payment }) =>
        Object.freeze({
          kind,
          owner: expectedOwner(payment.recipient),
          mint: payment.mint,
          amount: payment.amount,
        }),
      ),
    ),
    withdrawals: Object.freeze(await Promise.all(transfer.withdrawals.map(expectedWithdrawal))),
    allowed,
    outputTreeId,
  });
}

async function expectedWithdrawal(withdrawal: AuthorityWithdrawal): Promise<PublicLeg> {
  const { mint, recipient, amount, tokenProgram } = withdrawal;
  if (mint === SOL_MINT) {
    if (tokenProgram !== undefined) throw mismatch("Data");
    return withdrawalLeg({ kind: "sol", recipient }, amount);
  }
  if (tokenProgram === undefined) throw mismatch("Data");
  const [splTokenInterface] = await splInterfaceWithBump(mint);
  const recipientTokenAccount = await getAssociatedTokenAddress(recipient, mint, tokenProgram);
  return withdrawalLeg(
    { kind: "spl", mint, splTokenInterface, recipientTokenAccount, tokenProgram },
    amount,
  );
}

/** A withdrawal's settlement group in SPP's order. */
export function withdrawalLeg(withdrawal: TransactWithdrawal, amount: bigint): PublicLeg {
  const direction = "withdrawal";
  if (withdrawal.kind === "sol")
    return Object.freeze({
      direction,
      mint: null,
      amount,
      accounts: Object.freeze([SOL_INTERFACE, withdrawal.recipient]),
    });
  const { mint, splTokenInterface, recipientTokenAccount, tokenProgram } = withdrawal;
  return Object.freeze({
    direction,
    mint,
    amount,
    accounts: Object.freeze([
      SHIELDED_POOL_CPI_AUTHORITY,
      mint,
      splTokenInterface,
      recipientTokenAccount,
      tokenProgram,
    ]),
  });
}

function expectedOwner(address: ShieldedAddress): ExpectedOwner {
  return Object.freeze({
    ownerKey: address.signingPublicKey.confidentialViewTag(),
    nullifierPk: address.nullifierPublicKey,
    viewingU: address.viewingPublicKey.u(),
  });
}

/** Rust's serde JSON byte for byte, an amount past the safe integers stays exact. */
export function encodeRelayTransferTerms(terms: RelayTransferTerms): string {
  return JSON.stringify({
    ring: terms.ring,
    feePayer: terms.feePayer,
    signer: terms.signer,
    slots: terms.slots.map((opening) => ({
      slot: opening.slot,
      owner: opening.owner,
      nullifierPk: encodeBase58(opening.nullifierPk),
      mint: opening.mint,
      amount: rawJson(opening.amount.toString()),
      blinding: encodeBase58(opening.blinding),
      treeId: opening.treeId,
      dataHash: encodeBase58(opening.dataHash),
      ringDataHash: encodeBase58(opening.ringDataHash),
    })),
    nullifiers: terms.nullifiers.map(encodeBase58),
    legs: terms.legs.map(encodeLeg),
    setup: terms.setup,
  });
}

function encodeLeg(leg: PublicLeg): unknown {
  return {
    direction: leg.direction,
    mint: leg.mint,
    amount: rawJson(leg.amount.toString()),
    accounts: leg.accounts,
  };
}

/** An integer past the safe range keeps its digits, Serde refuses `7.0` and `-0` for an integer. */
function parseWire(json: string): unknown {
  return JSON.parse(json, (_key: string, value: unknown, context?: { source?: string }) => {
    if (typeof value !== "number") return value;
    const source = context?.source ?? "";
    if (!/^(0|[1-9][0-9]*)$/.test(source)) throw mismatch("Undecodable");
    return Number.isSafeInteger(value) ? value : source;
  }) as unknown;
}

function decodeTokenAccountCreate(entry: unknown): TokenAccountCreate {
  const create = fields(entry, ["owner", "mint", "tokenProgram"]);
  return Object.freeze({
    owner: address(create["owner"] as string),
    mint: address(create["mint"] as string),
    tokenProgram: address(create["tokenProgram"] as string),
  });
}

export function decodeRelayTransferTerms(json: string): RelayTransferTerms {
  return withMismatch("Undecodable", () => {
    const wire = parseWire(json);
    const row = fields(wire, [
      "ring",
      "feePayer",
      "signer",
      "slots",
      "nullifiers",
      "legs",
      "setup",
    ]);
    return Object.freeze({
      ring: address(row["ring"] as string),
      feePayer: address(row["feePayer"] as string),
      signer: address(row["signer"] as string),
      slots: (row["slots"] as readonly unknown[]).map(decodeSlotOpening),
      nullifiers: (row["nullifiers"] as readonly unknown[]).map((nullifier) =>
        scalar.hash(nullifier, "nullifiers"),
      ),
      legs: (row["legs"] as readonly unknown[]).map(decodeLeg),
      setup: (row["setup"] as readonly unknown[]).map(decodeTokenAccountCreate),
    });
  });
}

/** Rust's serde JSON byte for byte. */
export function encodeRelayFunderTerms(terms: RelayFunderTerms): string {
  return JSON.stringify({
    ring: terms.ring,
    feePayer: terms.feePayer,
    signer: terms.signer,
    legs: terms.legs.map(encodeLeg),
    setup: terms.setup,
  });
}

export function decodeRelayFunderTerms(json: string): RelayFunderTerms {
  return withMismatch("Undecodable", () => {
    const row = fields(parseWire(json), ["ring", "feePayer", "signer", "legs", "setup"]);
    return Object.freeze({
      ring: address(row["ring"] as string),
      feePayer: address(row["feePayer"] as string),
      signer: address(row["signer"] as string),
      legs: (row["legs"] as readonly unknown[]).map(decodeLeg),
      setup: (row["setup"] as readonly unknown[]).map(decodeTokenAccountCreate),
    });
  });
}

function decodeLeg(entry: unknown): PublicLeg {
  const leg = fields(entry, ["direction", "mint", "amount", "accounts"]);
  const direction = leg["direction"];
  if (direction !== "deposit" && direction !== "withdrawal") throw mismatch("Undecodable");
  return Object.freeze({
    direction,
    mint: leg["mint"] === null ? null : address(leg["mint"] as string),
    amount: scalar.u64(leg["amount"], "amount"),
    accounts: Object.freeze(
      (leg["accounts"] as readonly unknown[]).map((account) => address(account as string)),
    ),
  });
}

function decodeSlotOpening(value: unknown): SlotOpening {
  const row = fields(value, [
    "slot",
    "owner",
    "nullifierPk",
    "mint",
    "amount",
    "blinding",
    "treeId",
    "dataHash",
    "ringDataHash",
  ]);
  return Object.freeze({
    slot: scalar.count(row["slot"], "slot", 0xffff_ffff),
    owner: address(row["owner"] as string),
    nullifierPk: scalar.hash(row["nullifierPk"], "nullifierPk"),
    mint: address(row["mint"] as string),
    amount: scalar.u64(row["amount"], "amount"),
    blinding: scalar.hash(row["blinding"], "blinding"),
    treeId: scalar.count(row["treeId"], "treeId", 0xffff),
    dataHash: scalar.hash(row["dataHash"], "dataHash"),
    ringDataHash: scalar.hash(row["ringDataHash"], "ringDataHash"),
  });
}

/** Rust `deny_unknown_fields` with every field required. */
function fields(value: unknown, names: readonly string[]): Readonly<Record<string, unknown>> {
  if (typeof value !== "object" || value === null) throw mismatch("Undecodable");
  const present = Object.keys(value);
  if (present.length !== names.length || !names.every((name) => present.includes(name)))
    throw mismatch("Undecodable");
  return value as Readonly<Record<string, unknown>>;
}

const scalar = wirePrimitives(() => {
  throw mismatch("Undecodable");
});

/** Rust serde JSON byte for byte, `RelayTransferTerms` hands nothing of it over. */
export function encodeApprovalOpening(opening: ApprovalOpening): string {
  return JSON.stringify({
    blinder: encodeBase58(opening.blinder),
    outputs: opening.outputs.map((slot) => ({
      asset: encodeBase58(slot.asset),
      peak: slot.peak,
      chargedOutflow: slot.chargedOutflow,
    })),
    public: opening.public.map((slot) => ({
      asset: encodeBase58(slot.asset),
      peak: slot.peak,
      outflow: slot.outflow,
      gross: slot.gross,
    })),
  });
}

export function decodeApprovalOpening(json: string): ApprovalOpening {
  return withMismatch("Undecodable", () => decodedApprovalOpening(JSON.parse(json)));
}

/** Rust `deny_unknown_fields`, an amount reads as text and is parsed at the check. */
function decodedApprovalOpening(wire: unknown): ApprovalOpening {
  const row = fields(wire, ["blinder", "outputs", "public"]);
  return Object.freeze({
    blinder: scalar.hash(row["blinder"], "blinder"),
    outputs: Object.freeze(
      (row["outputs"] as readonly unknown[]).map((entry): OutputTotalsOpening => {
        const slot = fields(entry, ["asset", "peak", "chargedOutflow"]);
        return Object.freeze({
          asset: scalar.hash(slot["asset"], "asset"),
          peak: scalar.text(slot["peak"], "peak"),
          chargedOutflow: scalar.text(slot["chargedOutflow"], "chargedOutflow"),
        });
      }),
    ),
    public: Object.freeze(
      (row["public"] as readonly unknown[]).map((entry): PublicTotalsOpening => {
        const slot = fields(entry, ["asset", "peak", "outflow", "gross"]);
        return Object.freeze({
          asset: scalar.hash(slot["asset"], "asset"),
          peak: scalar.text(slot["peak"], "peak"),
          outflow: scalar.text(slot["outflow"], "outflow"),
          gross: scalar.text(slot["gross"], "gross"),
        });
      }),
    ),
  });
}

/** The ring account and the creates a relay puts ahead of the ring instruction. */
async function frameSetup(
  ring: Address,
  feePayer: Address,
  creates: readonly TokenAccountCreate[],
): Promise<Readonly<{ policy: Address; setup: readonly Instruction[] }>> {
  const [policy, setup] = await Promise.all([
    ringPolicyAddress(ring),
    Promise.all(
      creates.map((create) =>
        getCreateAssociatedTokenAccountInstructionAsync({
          payer: feePayer,
          owner: create.owner,
          mint: create.mint,
          tokenProgram: create.tokenProgram,
        }),
      ),
    ),
  ]);
  return { policy, setup: Object.freeze(setup) };
}

export async function expectedPublicOwner(
  input: Readonly<{
    terms: RelayTransferTerms;
    config: RingPolicy;
    knownNotes: readonly Pick<WalletUtxo, "nullifier">[];
  }>,
): Promise<ExpectedPublicOwner> {
  const { terms, config } = input;
  return Object.freeze({
    kind: "publicOwner",
    terms,
    ringPolicy: config,
    ringAuth: await ringAuthAddress(terms.ring),
    ...(await frameSetup(terms.ring, terms.feePayer, terms.setup)),
    rail: ringRailOf(RingInstructionTag.transfer, config.mode),
    knownNullifiers: Object.freeze(input.knownNotes.map((note) => note.nullifier)),
  });
}

export async function expectedFunder(
  input: Readonly<{ terms: RelayFunderTerms; config: RingPolicy }>,
): Promise<ExpectedFunder> {
  const { terms, config } = input;
  return Object.freeze({
    kind: "funder",
    terms,
    ringPolicy: config,
    ringAuth: await ringAuthAddress(terms.ring),
    ...(await frameSetup(terms.ring, terms.feePayer, terms.setup)),
    rail: ringRailOf(RingInstructionTag.transfer, config.mode),
  });
}

export async function expectedCoSigner(
  input: Readonly<{
    ring: Address;
    feePayer: Address;
    signer: Address;
    config: RingPolicy;
    setup: readonly TokenAccountCreate[];
    rule: CoSignerRule;
  }>,
): Promise<ExpectedCoSigner> {
  return Object.freeze({
    kind: "coSigner",
    ring: input.ring,
    ringAuth: await ringAuthAddress(input.ring),
    ringPolicy: input.config,
    feePayer: input.feePayer,
    signer: input.signer,
    record: await ringCoSignerAddress(input.ring, input.signer),
    ...(await frameSetup(input.ring, input.feePayer, input.setup)),
    rail: ringRailOf(RingInstructionTag.transfer, input.config.mode),
    rule: input.rule,
  });
}

export function checkRelayMessage(
  expected: ExpectedRelayMessage,
  message: Uint8Array,
  signer: Address,
): VerifiedRelayMessage {
  const snapshot = message.slice();
  if (expected.kind === "sealed") throw mismatch("Unrequested");
  const envelope = frameEnvelope(expected, signer, frameTag(expected));
  const { data, accounts } = open(envelope, snapshot);
  checkFrame(expected, envelope, data, accounts);
  return new VerifiedRelayMessage(VERIFIED, envelope.signer, snapshot);
}

function frameTag(expected: ExpectedFrame): RingInstructionTag {
  return expected.kind === "delegate"
    ? RingInstructionTag.authorityTransfer
    : RingInstructionTag.transfer;
}

function frameEnvelope(
  expected: ExpectedFrame,
  signer: Address,
  tag: RingInstructionTag,
): Envelope {
  if (expected.kind === "delegate") {
    if (expected.delegate !== signer) throw mismatch("Unrequested");
    return {
      program: expected.ring,
      policy: expected.policy,
      units: ringComputeUnitLimit(tag),
      feePayer: expected.feePayer,
      signer: expected.delegate,
      setup: [],
    };
  }
  if (expected.kind === "coSigner") {
    if (expected.signer !== signer) throw mismatch("Unrequested");
    return {
      program: expected.ring,
      policy: expected.policy,
      units: ringComputeUnitLimit(tag),
      feePayer: expected.feePayer,
      signer,
      setup: expected.setup,
    };
  }
  const { terms, policy, setup } = expected;
  const empty =
    expected.kind === "funder"
      ? expected.terms.legs.length === 0
      : expected.terms.slots.length === 0 && expected.terms.nullifiers.length === 0;
  if (terms.signer !== signer || empty) throw mismatch("Unrequested");
  return {
    program: terms.ring,
    policy,
    units: ringComputeUnitLimit(tag),
    feePayer: terms.feePayer,
    signer,
    setup,
  };
}

interface FrameAccounts {
  readonly accounts: readonly AccountMeta[];
  readonly layout: TransferLayout;
  readonly legs: readonly SettlementLeg[];
}

/** The one copy of the ring auth PDA fixes SPP's window, every other window follows the frame. */
function frameAccounts(
  expected: ExpectedFrame,
  frame: RingTransactData,
  accounts: readonly AccountMeta[],
): FrameAccounts {
  const copies = accounts.flatMap((meta, at) => (meta.address === expected.ringAuth ? [at] : []));
  const [slot] = copies;
  if (slot === undefined || copies.length !== 1) throw mismatch("Accounts");
  return withMismatch("Accounts", () => {
    const registries = transferRegistryCount(expected.ringPolicy, frame.rail);
    const prefix = knownPrefixEndingAt(frame, registries, slot - RING_TRANSACT_RING_CONFIG_INDEX);
    const layout = transferLayout(frame, registries, accounts.length, prefix);
    return { accounts, layout, legs: settlementLegs(layout, frame.interfaceTransfers) };
  });
}

function slotsNaming(
  { accounts }: FrameAccounts,
  key: Address,
  { start, end }: AccountWindow = { start: 0, end: accounts.length },
): readonly number[] {
  return accounts.flatMap((meta, at) =>
    at >= start && at < end && meta.address === key ? [at] : [],
  );
}

/** A signature authorizes every slot naming its key, so the key sits at `allowed` alone. */
function checkSignerSlots(
  envelope: Envelope,
  decoded: FrameAccounts,
  allowed: readonly number[],
  writes: "refused" | "accepted" = "refused",
): void {
  const named = slotsNaming(decoded, envelope.signer);
  const held =
    allowed.length > 0 &&
    named.length === allowed.length &&
    named.every((slot, at) => {
      const role = decoded.accounts[slot]?.role;
      return (
        slot === allowed[at] &&
        role !== undefined &&
        isSignerRole(role) &&
        (writes === "accepted" || !isWritableRole(role))
      );
    });
  if (!held || !readsOnly(envelope, decoded)) throw mismatch("Accounts");
}

/** One slot alone, a second entry of the key would stand for a signature the message holds once. */
function checkSoleSigner(
  envelope: Envelope,
  decoded: FrameAccounts,
  slots: readonly number[],
): void {
  if (slots.length !== 1) throw mismatch("Accounts");
  checkSignerSlots(envelope, decoded, slots);
}

/** Direction, amount, mint and the whole settlement group in order. */
function bindsLeg(
  named: PublicLeg | undefined,
  leg: SettlementLeg,
  accounts: readonly AccountMeta[],
): boolean {
  const group = accounts.slice(leg.accounts.start, leg.accounts.end);
  const mintAt = mintAccountPosition(leg.transfer);
  const mint = mintAt === undefined ? null : group[mintAt]?.address;
  return (
    named !== undefined &&
    named.direction === (isDepositLeg(leg.transfer) ? "deposit" : "withdrawal") &&
    named.amount === leg.transfer.amount &&
    named.mint === mint &&
    named.accounts.length === group.length &&
    group.every((meta, at) => meta.address === named.accounts[at])
  );
}

/** One to one in frame order, a leg the sender adds, drops or moves is refused. */
function checkLegs(
  named: readonly PublicLeg[],
  legs: readonly SettlementLeg[],
  accounts: readonly AccountMeta[],
): void {
  if (named.length !== legs.length || !legs.every((leg, at) => bindsLeg(named[at], leg, accounts)))
    throw mismatch("Legs");
}

function checkFrame(
  expected: ExpectedFrame,
  envelope: Envelope,
  data: Uint8Array,
  accounts: readonly AccountMeta[],
): void {
  const tag = frameTag(expected);
  if (data[0] !== tag) throw mismatch("Data");
  const frame = withMismatch("Data", () => RingTransactData.parse(data.subarray(1), expected.rail));
  const decoded = frameAccounts(expected, frame, accounts);
  const { layout } = decoded;
  switch (expected.kind) {
    case "funder":
      return checkFunder(expected, envelope, decoded);
    case "delegate":
      checkMoved(expected, frame, decoded);
      return checkSoleSigner(
        envelope,
        decoded,
        layout.delegate === undefined ? [] : [layout.delegate],
      );
    case "coSigner":
      return checkCoSigner(expected, frame, envelope, decoded);
    case "publicOwner": {
      checkOpenings(expected, frame, decoded);
      return checkSoleSigner(
        envelope,
        decoded,
        slotsNaming(decoded, envelope.signer, layout.owners),
      );
    }
  }
}

/** The proof binds each record to its slot under the key the frame publishes. */
function checkMoved(
  expected: ExpectedDelegate,
  frame: RingTransactData,
  decoded: FrameAccounts,
): void {
  // Owner tags alone, the message holds no blinding, a hidden frame's tags bind nothing.
  checkTransactionKey(expected.txViewingKey, frame);
  const { envelope, recipientKeys } = openStream(expected.txViewingKey, expected.auditor, frame);
  if (frame.hasData || frame.ringData) throw mismatch("Data");
  checkLegs(expected.withdrawals, decoded.legs, decoded.accounts);
  // 2. Matching outputs bind no input, every proven spend answers to the requested openings.
  if (expected.allowed?.countsSpends(envelope.realInputs, frame.nullifiers) === false)
    throw mismatch("SpendsUnrequestedNote");
  const records = frameRecords(expected.txViewingKey, frame, recipientKeys);
  // 3. Each live record pays one requested payment or returns change to a requested source,
  // sealed to that owner's viewing key so the owner can open it. A ring output's record names no
  // nonzero key, the rebuilt commitment alone binds the output to the owner's nullifier key.
  const owns = (owner: ExpectedOwner, record: OutputRecord): boolean => {
    if (!equalBytes(record.ownerKey, owner.ownerKey)) return false;
    const zero = equalBytes(owner.nullifierPk, ZERO_NULLIFIER_PK);
    switch (record.key.kind) {
      case "zero":
        return zero;
      case "unpublished":
        return !zero;
      case "carried":
        return equalBytes(record.key.nullifierPk, owner.nullifierPk);
    }
  };
  const unpaid = [...expected.payments];
  records.forEach((record, slot) => {
    if (record.kind === RecordKind.dummy) return;
    const published = frame.outputs[slot];
    const sealedTo = (owner: ExpectedOwner): boolean =>
      (record.kind === RecordKind.exit ||
        (recipientKeys[slot] !== undefined && equalBytes(recipientKeys[slot], owner.viewingU))) &&
      published !== undefined &&
      rebuilds(expected, published.hash, record, owner);
    const paid = unpaid.findIndex(
      (payment) =>
        record.kind === payment.kind &&
        record.mint === payment.mint &&
        record.amount === payment.amount &&
        owns(payment.owner, record) &&
        sealedTo(payment.owner),
    );
    const source =
      paid < 0 && record.kind === RecordKind.ring
        ? expected.sources.find((source) => owns(source, record) && sealedTo(source))
        : undefined;
    const owner = unpaid[paid]?.owner ?? source;
    if (owner === undefined) throw mismatch("Recipients");
    if (paid >= 0) {
      // Rust `swap_remove`, the order decides the equal payment a later record takes.
      unpaid[paid] = unpaid[unpaid.length - 1] as ExpectedPayment;
      unpaid.pop();
    }
    const tag = frame.outputs[slot]?.ownerTag;
    // A confidential reader finds its note by the tag, a change under another tag is lost to its source.
    const expectedTag =
      record.kind === RecordKind.exit
        ? owner.ownerKey
        : !isAnonymousFrame(frame.rail)
          ? slotTag(owner.viewingU)
          : undefined;
    if (expectedTag !== undefined && (tag === undefined || !equalBytes(tag, expectedTag)))
      throw mismatch("Recipients");
  });
  if (unpaid.length !== 0) throw mismatch("Recipients");
}

/** The rail carries no data, so both data hashes are zero, Rust `ExpectedDelegate::rebuilds`. */
function rebuilds(
  expected: ExpectedDelegate,
  hash: Bytes32,
  record: OutputRecord,
  owner: ExpectedOwner,
): boolean {
  const opening: SlotOpening = {
    slot: 0,
    owner: encodeBase58(record.ownerKey) as Address,
    nullifierPk: owner.nullifierPk,
    mint: record.mint,
    amount: record.amount,
    blinding: record.blinding,
    treeId: expected.outputTreeId,
    dataHash: ZERO_32,
    ringDataHash: ZERO_32,
  };
  try {
    return equalBytes(
      openingHash(opening, record.kind === RecordKind.ring ? expected.ring : undefined),
      hash,
    );
  } catch {
    return false;
  }
}

function checkTransactionKey(txViewingKey: Bytes32, frame: RingTransactData): void {
  const key = withMismatch("TransactionKey", () =>
    canonicalViewingKey(txViewingKey.slice() as Bytes32),
  );
  const published = key.publicKey().u();
  key.destroy();
  if (!equalBytes(frame.txViewingU, published)) throw mismatch("TransactionKey");
}

function openStream(
  txViewingKey: Bytes32,
  auditor: X25519PublicKey,
  frame: RingTransactData,
): RingDisclosure {
  return withMismatch("Data", () => {
    const opened = openSealedDisclosure(txViewingKey, auditor, {
      layout: RingTransactData.streamLayout(frame),
      salt: frame.salt,
      message: {
        ephemeralPublicKey: x25519FromU(frame.txViewingU),
        envelope: frame.envelope,
        recipientKeys: frame.outputs.map((_, slot) =>
          frame.recipientKeys.subarray(slot * RECIPIENT_KEY_LEN, (slot + 1) * RECIPIENT_KEY_LEN),
        ),
      },
    });
    const { envelope } = opened;
    canonicalViewingKey(envelope.txViewingSecret.slice() as Bytes32).destroy();
    envelope.txViewingSecret.fill(0);
    if (
      envelope.evidence.kind === "entry" &&
      !envelope.evidence.inputs.every(
        (input) => isCanonicalField(input.nullifierPk) && isCanonicalField(input.inputCommitment),
      )
    )
      throw mismatch("Data");
    return opened;
  });
}

/** @internal Every slot's record of `frame` under the transaction key that sealed it. */
export function frameOutputRecords(
  txViewingKey: Bytes32,
  auditor: X25519PublicKey,
  frame: RingTransactData,
): readonly OutputRecord[] {
  const { recipientKeys } = openStream(txViewingKey, auditor, frame);
  return frameRecords(txViewingKey, frame, recipientKeys);
}

/** Each slot's record under `CRDS_i`, the proof binds it to `V_i` and the slot's commitment. */
function frameRecords(
  txViewingKey: Bytes32,
  frame: RingTransactData,
  recipientKeys: readonly Bytes32[],
): readonly OutputRecord[] {
  return withMismatch("Data", () => {
    const key = canonicalViewingKey(txViewingKey.slice() as Bytes32);
    try {
      return frame.outputs.map((output, slot) => {
        const recipient = recipientKeys[slot];
        if (recipient === undefined) throw mismatch("Data");
        const secret = txKeySlotSecret({
          txViewingKey: key,
          recipientViewingKey: x25519FromU(recipient),
          salt: frame.salt,
          outputIndex: slot,
          firstNullifier: RingTransactData.firstNullifier(frame),
        });
        try {
          const form = RingTransactData.recordForm(frame);
          return output.body.kind === "exit"
            ? openExitRecord(secret, output.record, form)
            : openOutputSlot(secret, output.record, form).record;
        } finally {
          secret.fill(0);
        }
      });
    } finally {
      key.destroy();
    }
  });
}

/**
 * SPP reads one signature as approval of every data-bearing output its signer owns, and each
 * published owner tag of a ring output is zero, so no frame field names those outputs. The signer
 * reads every published output before it signs, no data output stays out of its view.
 */
function checkOpenings(
  expected: ExpectedPublicOwner,
  frame: RingTransactData,
  decoded: FrameAccounts,
): void {
  const { terms, knownNullifiers } = expected;
  // 1. One opening per published slot in slot order, a missing or repeated slot could hide an
  // output the signature approves.
  if (
    terms.slots.length !== frame.outputs.length ||
    terms.slots.some((opening, slot) => opening.slot !== slot)
  )
    throw mismatch("UnopenedOutput");
  // 2. Each opening rebuilds the commitment published at its slot, so the signer reads each owner,
  // amount and data hash the proof commits.
  terms.slots.forEach((opening, slot) => {
    const published = frame.outputs[slot];
    if (
      published === undefined ||
      !equalBytes(
        // An exit commits no ring, a ring id here would refuse every exit.
        withMismatch("Recipients", () =>
          openingHash(opening, published.body.kind === "exit" ? undefined : terms.ring),
        ),
        published.hash,
      )
    )
      throw mismatch("Recipients");
  });
  const among = (list: readonly Uint8Array[], nullifier: Uint8Array): boolean =>
    list.some((held) => equalBytes(held, nullifier));
  // 3. A named note the wallet cannot place means no list stood behind the check.
  if (
    !terms.nullifiers.every(
      (named) => among(frame.nullifiers, named) && among(knownNullifiers, named),
    )
  )
    throw mismatch("Data");
  if (
    frame.nullifiers.some(
      (spent) => among(knownNullifiers, spent) && !among(terms.nullifiers, spent),
    )
  )
    throw mismatch("SpendsUnnamedNote");
  // 4. Every public leg is one the terms name, a leg moves funds the proof alone does not show the signer.
  checkLegs(terms.legs, decoded.legs, decoded.accounts);
}

function checkCoSigner(
  expected: ExpectedCoSigner,
  frame: RingTransactData,
  envelope: Envelope,
  decoded: FrameAccounts,
): void {
  const { rule } = expected;
  if (rule.kind === "exact") checkExactApproval(rule.approval, frame);
  else if ((frame.approval & ~rule.accepts) !== 0) throw mismatch("ApprovalMaskUnaccepted");
  // The key signs the pair of its own record, a slot beside another record approves for it.
  const own = coSignerPairs(decoded.layout).filter(({ record, signer }) => {
    const [recordMeta, signerMeta] = [decoded.accounts[record], decoded.accounts[signer]];
    return (
      recordMeta?.address === expected.record &&
      recordMeta.role === AccountRole.READONLY &&
      signerMeta?.address === envelope.signer &&
      signerMeta.role === AccountRole.READONLY_SIGNER
    );
  });
  checkSoleSigner(
    envelope,
    decoded,
    own.map((pair) => pair.signer),
  );
}

/**
 * The digest binds the totals, the limits then read them with no slot opened. A ring with no own
 * registry runs no outflow section and its circuit refuses a nonzero digest, such a frame reads as
 * `MissingApprovalDigest` and exact approval stays out of reach there, the proven bits remain the
 * only rule a co-signer can apply to it.
 */
function checkExactApproval(exact: ExactApproval, frame: RingTransactData): void {
  const { opening, limits, seenBlinders } = exact;
  const digest = frame.extensions.approvalTotals;
  if (digest === undefined) throw mismatch("MissingApprovalDigest");
  // 1. The chain hashes a flat run of three fields per output and four per public slot with no
  // length in it, so `4a` more output rows and `3a` fewer public rows rebuild the same digest with
  // every column shifted. Only the key's exact counts make the limits below read a peak as a peak,
  // and they hold ahead of hashing since the rows come from the sealing wallet.
  if (opening.outputs.length !== frame.nOut || opening.public.length !== N_PUBLIC_SLOTS)
    throw mismatch("ApprovalOpeningLayout");
  // 2. The circuit accepts any blinder, and a zero one lets anyone test guessed totals against the
  // public digest. Freshness and hiding rest on the reader's refusal alone.
  if (isZero(opening.blinder)) throw mismatch("ZeroApprovalBlinder");
  if (seenBlinders.some((seen) => equalBytes(seen, opening.blinder)))
    throw mismatch("ReusedApprovalBlinder");
  // 3. Rust's hasher errors at or past the modulus. A value the TS hasher reduced would open the
  // digest under a blinder `seenBlinders` does not hold, so each input is held canonical here.
  const totals = openingTotals(opening);
  if (
    totals === undefined ||
    !isCanonicalField(opening.blinder) ||
    ![...totals.outputs, ...totals.publicSlots].every((slot) => isCanonicalField(slot.asset)) ||
    !equalBytes(
      withMismatch("FalseApprovalOpening", () => totals.commitment(opening.blinder)),
      digest,
    )
  )
    throw mismatch("FalseApprovalOpening");
  // 4. The commitment bound `totals`, every nonzero asset they name now needs a limit.
  for (const slot of [...totals.outputs, ...totals.publicSlots]) {
    if (isZero(slot.asset)) continue;
    const limit = withMismatch("ApprovalOverLimit", () =>
      limits.find((entry) => equalBytes(memberOfAsset(entry.mint), slot.asset)),
    );
    // A deposit the sender funds offsets its own withdrawal in peak and outflow, a public row's
    // gross alone shows what leaves through it.
    if (
      limit === undefined ||
      totals.publicSlots.some(
        (row) => equalBytes(row.asset, slot.asset) && row.gross > limit.gross,
      ) ||
      totals.peakOf(slot.asset) > limit.peak ||
      totals.outflowOf(slot.asset) > limit.outflow
    )
      throw mismatch("ApprovalOverLimit");
  }
}

/** A run entry would authorize data outputs and spent notes the funder's terms never name. */
function checkFunder(expected: ExpectedFunder, envelope: Envelope, decoded: FrameAccounts): void {
  const { terms } = expected;
  const funded = decoded.legs.filter(
    (leg) => leg.funder !== undefined && decoded.accounts[leg.funder]?.address === terms.signer,
  );
  checkLegs(terms.legs, funded, decoded.accounts);
  // SPP debits the funder of a SOL leg, one write bit covers every slot of the key.
  const solFunded = terms.legs.some((leg) => leg.mint === null);
  checkSignerSlots(
    envelope,
    decoded,
    funded.flatMap((leg) => (leg.funder === undefined ? [] : [leg.funder])),
    solFunded ? "accepted" : "refused",
  );
}

export function openingHash(opening: SlotOpening, ring: Address | undefined): Bytes32 {
  if (opening.owner === ZERO_ADDRESS) return dummyOpeningHash(opening);
  return new Utxo({
    owner: ShieldedPublicKey.fromEd25519(addressBytes(opening.owner) as Bytes32),
    asset: opening.mint,
    amount: opening.amount,
    blinding: opening.blinding,
    ...(ring === undefined ? {} : { ringProgramId: ring }),
  })
    .proofInput(opening.nullifierPk, opening.treeId, opening.dataHash, opening.ringDataHash)
    .hash();
}

/** SPP pays no zero owner and holds a dummy's other fields to zero. */
function dummyOpeningHash(opening: SlotOpening): Bytes32 {
  if (
    opening.mint !== ZERO_ADDRESS ||
    opening.amount !== 0n ||
    [opening.nullifierPk, opening.dataHash, opening.ringDataHash].some((field) => !isZero(field))
  )
    throw mismatch("Recipients");
  return dummyUtxoHash(opening.treeId, opening.blinding);
}

/** The owner and mint of a dummy slot opening. */
export const ZERO_ADDRESS = encodeBase58(new Uint8Array(32)) as Address;

export function mismatch(reason: RelayMessageMismatch): RingError {
  return new RingError("RING_RELAY_MESSAGE_INVALID", { details: { mismatch: reason } });
}

interface Envelope {
  readonly program: Address;
  readonly policy: Address;
  readonly units: number;
  readonly feePayer: Address;
  readonly signer: Address;
  /** Ahead of the ring instruction, exactly these. */
  readonly setup: readonly Instruction[];
}

interface RingInstruction {
  readonly data: Uint8Array;
  readonly accounts: readonly AccountMeta[];
}

/** One canonical v1 message the quoted fee payer pays, holding only the ring instruction. */
function open(envelope: Envelope, bytes: Uint8Array): RingInstruction {
  if (bytes.length > TRANSACTION_SIZE_LIMIT) throw mismatch("Undecodable");
  const compiled = withMismatch("Undecodable", () =>
    getCompiledTransactionMessageDecoder().decode(bytes),
  );
  if (compiled.version !== 1) throw mismatch("NotV1");
  const canonical = withMismatch(
    "Undecodable",
    () => new Uint8Array(getCompiledTransactionMessageEncoder().encode(compiled)),
  );
  // Mirrors Rust `v1::Message::validate`, the vectors pin both on the same verdict.
  const { header, staticAccounts, instructionHeaders } = compiled;
  if (
    !equalBytes(canonical, bytes) ||
    staticAccounts.length > MAX_TRANSACTION_ADDRESSES ||
    new Set(staticAccounts).size !== staticAccounts.length ||
    header.numSignerAccounts > MAX_TRANSACTION_SIGNATURES ||
    header.numReadonlySignerAccounts >= header.numSignerAccounts ||
    instructionHeaders.length > MAX_TRANSACTION_INSTRUCTIONS ||
    instructionHeaders.some((instruction) => instruction.programAccountIndex === 0)
  )
    throw mismatch("Undecodable");
  const message = withMismatch("Undecodable", () =>
    decompileTransactionMessage(
      compiled as Parameters<typeof decompileTransactionMessage>[0] & { version: 1 },
    ),
  );
  if (message.feePayer.address !== envelope.feePayer || envelope.feePayer === envelope.signer)
    throw mismatch("FeePayer");
  const config = message.config ?? {};
  const withinBudget =
    config.computeUnitLimit !== undefined &&
    config.computeUnitLimit <= envelope.units &&
    config.loadedAccountsDataSizeLimit !== undefined &&
    config.loadedAccountsDataSizeLimit <= LOADED_ACCOUNTS_DATA_SIZE_LIMIT &&
    config.heapSize === undefined;
  if (!withinBudget) throw mismatch("Budget");
  const instruction = message.instructions.at(-1);
  const leading = message.instructions.slice(0, -1);
  if (instruction === undefined) throw mismatch("Instructions");
  // In named order, an omitted create costs the signer nothing, a transfer to a missing account only fails.
  const created = new Set<Address>();
  let next = 0;
  const sameSetup = leading.every((found) => {
    const accounts = found.accounts ?? [];
    for (; next < envelope.setup.length; next += 1) {
      const named = envelope.setup[next];
      if (named === undefined) break;
      const namedAccounts = named.accounts ?? [];
      const account = namedAccounts[1]?.address;
      if (
        account !== undefined &&
        found.programAddress === named.programAddress &&
        equalBytes(new Uint8Array(found.data ?? []), new Uint8Array(named.data ?? [])) &&
        accounts.length === namedAccounts.length &&
        accounts.every((meta, at) => meta.address === namedAccounts[at]?.address)
      ) {
        created.add(account);
        next += 1;
        return true;
      }
    }
    return false;
  });
  if (!sameSetup) throw mismatch("Instructions");
  if (instruction.programAddress !== envelope.program) throw mismatch("Program");
  const ringAccounts = instruction.accounts ?? [];
  const inRing = new Set(ringAccounts.map((meta) => meta.address));
  const writableSigners = header.numSignerAccounts - header.numReadonlySignerAccounts;
  const writableEnd = staticAccounts.length - header.numReadonlyNonSignerAccounts;
  // Outside the ring instruction only the fee payer signs and only a matched create's token account is written.
  for (const [index, pubkey] of staticAccounts.entries()) {
    if (pubkey === envelope.feePayer || inRing.has(pubkey)) continue;
    const signerRole = index < header.numSignerAccounts;
    const writableRole = signerRole ? index < writableSigners : index < writableEnd;
    if (signerRole || (writableRole && !created.has(pubkey))) throw mismatch("Accounts");
  }
  return {
    data: new Uint8Array(instruction.data ?? []),
    accounts: ringAccounts,
  };
}

/** Compiled v1 messages share one write bit per address across prefix and forwarded. */
function readsOnly(envelope: Envelope, { accounts, layout }: FrameAccounts): boolean {
  const [ring, ...rest] = accounts.slice(0, layout.spp);
  const forwardedWrite = (meta: AccountMeta): boolean =>
    accounts
      .slice(layout.spp)
      .some((spp) => spp.address === meta.address && isWritableRole(spp.role));
  return (
    ring !== undefined &&
    ring.address === envelope.policy &&
    !isSignerRole(ring.role) &&
    !isWritableRole(ring.role) &&
    rest.every((meta) => !isWritableRole(meta.role) || forwardedWrite(meta))
  );
}

function withMismatch<T>(reason: RelayMessageMismatch, run: () => T): T {
  try {
    return run();
  } catch (error) {
    if (error instanceof RingError && error.code === "RING_RELAY_MESSAGE_INVALID") throw error;
    throw mismatch(reason);
  }
}
