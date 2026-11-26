import type { Address, Instruction } from "@solana/kit";
import { getCreateAssociatedTokenAccountInstructionAsync } from "@heliuslabs/spr/instructions";
import {
  SOL_INTERFACE,
  SHIELDED_POOL_CPI_AUTHORITY,
  type Bytes32,
  type TransactWithdrawal,
} from "@heliuslabs/spr/interface";
import { getAssociatedTokenAddress } from "@heliuslabs/spr/addresses";
import { encodeBase58, equalBytes, splInterfaceWithBump } from "@heliuslabs/spr/internal";
import type { NullifierKey, ShieldedAddress, X25519PublicKey } from "@heliuslabs/spr/keypair";
import { ProofInputUtxo, SOL_MINT, type WalletUtxo } from "@heliuslabs/spr/transaction";
import { RingError } from "../../protocol/error.js";
import type { ApprovalOpening } from "../../protocol/membership/approval-totals.js";
import type { OwnerDebit } from "../../protocol/membership/owner-debits.js";
import {
  ringAuthAddress,
  ringCoSignerAddress,
  ringPolicyAddress,
} from "../../protocol/pda/index.js";
import type {
  AuthorityWithdrawal,
  PrepareAuthorityTransferRequest,
  RelayQuote,
} from "../../protocol/ring-rpc/ring-rpc-types.js";
import { RingInstructionTag } from "../../protocol/transaction/instruction-tag.js";
import { ringRailOf, type RingRail } from "../../protocol/transaction/transact.js";
import type { RingPolicy } from "../../protocol/codec/ring-codecs.js";
import { RecordKind } from "../../protocol/codec/audit-stream.js";

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

export interface ExpectedOwner {
  readonly ownerKey: Bytes32;
  readonly nullifierPk: Bytes32;
  readonly viewingU: Bytes32;
}

export interface ExpectedPayment {
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

export interface ApprovalLimit {
  readonly mint: Address;
  /** Bounds one owner's outflow. */
  readonly peak: bigint;
  /** Bounds the sum over owners. */
  readonly outflow: bigint;
  /** Bounds each public row's withdrawals with no deposit subtracted. */
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

export type ExpectedFrame =
  | ExpectedDelegate
  | ExpectedPublicOwner
  | ExpectedFunder
  | ExpectedCoSigner;

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

/** The owner and mint of a dummy slot opening. */
export const ZERO_ADDRESS = encodeBase58(new Uint8Array(32)) as Address;

export function mismatch(reason: RelayMessageMismatch): RingError {
  return new RingError("RING_RELAY_MESSAGE_INVALID", { details: { mismatch: reason } });
}

export function withMismatch<T>(reason: RelayMessageMismatch, run: () => T): T {
  try {
    return run();
  } catch (error) {
    if (error instanceof RingError && error.code === "RING_RELAY_MESSAGE_INVALID") throw error;
    throw mismatch(reason);
  }
}
