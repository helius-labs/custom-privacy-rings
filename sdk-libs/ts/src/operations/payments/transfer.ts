/** Ring transfers from a wallet, every flow selects, reserves, approves, proves and compiles through one driver. */
import type { Instruction } from "@solana/kit";

import type {
  BlockhashProvider,
  PreparedOwnerSignature,
  RingTransfers,
  WalletKeys,
} from "@heliuslabs/spr/client";
import {
  reserveEntries,
  reservedUtxoKeys,
  unreserved,
  resolveWithdrawalSettlement,
  withdrawalSetupInstructions,
  signerAddress,
  type SignerAccount,
  type SpendSelectionErrors,
  MAX_TRANSACT_INPUTS,
  ownerSolanaAccount,
  withdrawalIntentRecipient,
  equalBytes,
  isZero,
  resolveShieldedRecipient,
} from "@heliuslabs/spr/internal";
import { initializePoseidon } from "@heliuslabs/spr";
import type {
  Address,
  Bytes32,
  RequestContext,
  Transaction,
  TransactWithdrawal,
} from "@heliuslabs/spr/interface";
import { ShieldedAddress, type NullifierKey } from "@heliuslabs/spr/keypair";
import { SOL_MINT, ConfidentialTransfer, type PreparedTransfer } from "@heliuslabs/spr/transaction";
import {
  approveRingUnattended,
  checkRingIntentApproval,
  checkRingPreparedTransfer,
  ringIntentHash,
  type RingApprovalHandler,
  type RingTransactionIntent,
} from "../../protocol/transaction/intent.js";
import type { Wallet, WalletUtxo } from "@heliuslabs/spr/transaction";
import { ringSpendTransfer } from "./ring-spend.js";
import type { RingListSalt } from "../../protocol/membership/policy.js";
import { frameKeyShape, TRANSFER_SHAPES } from "../../protocol/proof/transfer-shapes.js";
import type { ReplyMetadata } from "../../protocol/codec/anonymous-delivery.js";
import { RingAddress, RingPaymentRequest } from "../../crypto/keys/ring-address.js";

import {
  carriesNoData,
  type AnonymousPayment,
  type AnonymousSpendSigner,
  type OwnerExposure,
} from "./anonymous.js";
import { fetchRingPolicy } from "../../protocol/membership/config.js";
import { RingError, wrapRingError } from "../../protocol/error.js";
import type { ProvenRingTransfer } from "../workflow/pipeline.js";
import type { SealRow } from "../workflow/transfer-plan.js";
import {
  CustomRingTransfer,
  type AnonymousTransfer,
  type AnonymousTransferClient,
  type CustomRingTransferClient,
  type CustomRingTransferInput,
  type PreparedRingRelay,
  type RingTransferApproval,
} from "./transfer-rails.js";
import type { RelayQuote, TokenAccountSetup } from "../../protocol/ring-rpc/ring-rpc-types.js";
import { isZeroKeyNote } from "../../state/exposed-notes.js";
import {
  ringIntentMismatch,
  ringProofInputs,
  ringSelectionErrors,
  selectFunding,
} from "../workflow/selection.js";
import {
  RingTransactionSubmission,
  checkRetainedEntries,
  provenComputeUnitLimit,
  provenRingAttempt,
  type RingSubmissionAttempt,
  type RingSubmissionBuildState,
} from "../workflow/submission.js";
import { resolveRingOutputTree } from "../../protocol/membership/trees.js";

export type RingTransferClient = CustomRingTransferClient & BlockhashProvider;

/** The owner signs the message hash inside the proof. */
export interface RingRelaySpend {
  /** Names the fee payer the transfer pays from. */
  readonly quote: RelayQuote;
  /** Ring RPC proves with it and so links every spend of the owner. */
  readonly nullifierKey: NullifierKey;
  /** Must hold the sender's signing key. */
  readonly signer: AnonymousSpendSigner;
  /** The row a Ring RPC reseal refusal names, unset seals under the public counts' row. */
  readonly sealRow?: SealRow;
}

export interface AnonymousSpendOwner {
  readonly signer: AnonymousSpendSigner;
  /** `null` sends no reply, no path attaches one by default. */
  readonly reply?: ReplyMetadata | null;
  /** A missing SPL exit token account must be the owner's associated token account. */
  readonly exitTokenOwner?: Address;
  /** `AnonymousTransfer.withOwnerExposure`, `refused` when unset. */
  readonly ownerExposure?: OwnerExposure;
}

export interface AnonymousRingSpend extends RingRelaySpend, AnonymousSpendOwner {}

/** What every ring spend names, each flow adds its recipient. */
export interface RingSpendParams {
  readonly client: RingTransferClient;
  readonly ringProgramId: Address;
  readonly wallet: Wallet;
  readonly keys: WalletKeys;
  readonly feePayer: Address;
  readonly approve?: RingApprovalHandler;
  readonly asset?: Address;
  readonly amount: bigint;
  /** Receives every private output, defaults to `client.tree`. */
  readonly outputTree?: Address;
  /** Read in order while a demanded class stays uncovered, each one read signs. */
  readonly cosigners?: readonly SignerAccount[];
  /** The sender's and any other enrolled owner's salt a salted list reads, a paid `RingAddress` adds its own. */
  readonly listSalts?: readonly RingListSalt[];
  /** A direct proof reads the private lists under it on a ring that commits its tag. */
  readonly ringSalt?: Bytes32;
  readonly priorityFeeLamports?: bigint;
}

/** Without `inProofOwner` each input owner signs the transaction. */
export type RingOwnerRail = Readonly<{
  inProofOwner?: AnonymousSpendSigner;
  anonymous?: never;
  spendPublic?: never;
}>;

/** The fee payer and any co-signer must differ from the hidden owner. */
export type RingAnonymousRail = Readonly<{
  anonymous: AnonymousSpendOwner;
  client: AnonymousTransferClient & BlockhashProvider;
  inProofOwner?: never;
  spendPublic?: never;
}>;

/** `wallet` becomes public, recipients stay hidden on an anonymous ring. */
export interface RingPublicSpend {
  readonly wallet: Address;
  /** `null` sends no reply, named on every anonymous payment. */
  readonly reply?: ReplyMetadata | null;
}

export type RingPublicOwnerRail = Readonly<{
  spendPublic: RingPublicSpend;
  inProofOwner?: never;
  anonymous?: never;
}>;

export type RingDirectRail = RingOwnerRail | RingAnonymousRail | RingPublicOwnerRail;

/** `anonymous` on an anonymous ring, `confidential` on any other. */
export type RingRelayRail =
  | RingAnonymousRelayRail
  | Readonly<{ confidential: RingRelaySpend; anonymous?: never }>;

export type RingAnonymousRelayRail = Readonly<{
  anonymous: AnonymousRingSpend;
  confidential?: never;
}>;

/** Ring RPC pays and co-signs a relayed spend. */
export type RingRelaySpendParams<
  P extends Omit<RingSpendParams, "amount"> = RingSpendParams,
  R extends RingRelayRail = RingRelayRail,
> = Omit<P, "feePayer" | "cosigners" | "priorityFeeLamports"> & R;

/** The owner signs after reading `relay.chargedOutflows()`. */
export type PreparedSpendRelay = PreparedRingRelay;

export interface RingTransferTransactionParams extends RingSpendParams {
  /** A ring address or payment request on an anonymous ring, a shielded address or a registered owner on any other. */
  readonly recipient: Address | ShieldedAddress | RingAddress | RingPaymentRequest;
  /** `"default"` funds only from default UTXOs. `"ring-or-default"` mixes both pools, an anonymous ring funds from one pool alone. */
  readonly inputs?: "ring" | "ring-or-default" | "default";
}

/** One payment of the transfer's asset. */
export interface RingTransferPayment {
  readonly recipient: RingTransferTransactionParams["recipient"];
  readonly amount: bigint;
}

/** Each payment takes one output, the inputs cover their sum. */
export interface RingMultiTransferParams extends Omit<
  RingTransferTransactionParams,
  "recipient" | "amount"
> {
  /** One to `MAX_RING_TRANSFER_PAYMENTS`, every one in `asset`. */
  readonly payments: readonly RingTransferPayment[];
  readonly recipient?: never;
  readonly amount?: never;
}

/** The widest transfer key's outputs less the change output. */
export const MAX_RING_TRANSFER_PAYMENTS =
  Math.max(...TRANSFER_SHAPES.map(({ outputs }) => outputs)) - 1;

export type RingEntryTransactionParams = RingSpendParams;

export interface RingExitTransactionParams extends RingSpendParams {
  readonly recipient: Address | ShieldedAddress;
}

export interface RingWithdrawalTransactionParams extends RingSpendParams {
  /** Any Solana account. It needs no registry record and need not exist yet. */
  readonly recipient: Address;
  /** SPL Token or Token-2022 for non-SOL assets, the settlement lands in the recipient's ATA. */
  readonly splTokenProgram?: Address;
}

/** Unsigned, the fee payer and any co-signer sign. */
/** Unsigned, the fee payer and any co-signer sign. */
export async function buildRingTransferTransaction(
  input: (RingTransferTransactionParams | RingMultiTransferParams) & RingOwnerRail,
  context?: RequestContext,
): Promise<Transaction> {
  return buildSpendTransaction(paidSpend(input), input, ringSend, context);
}

export function createRingTransferSubmission(
  input: (RingTransferTransactionParams | RingMultiTransferParams) & RingDirectRail,
  context?: RequestContext,
): Promise<RingTransactionSubmission> {
  return createSpendSubmission(paidSpend(input), input, ringSend, context);
}

/**
 * Value leaves the ring to a default-ring UTXO of the recipient, and the audit proof still covers
 * the exit. Only ring-bound UTXOs fund it.
 */
export async function buildRingExitTransaction(
  input: RingExitTransactionParams & RingOwnerRail,
  context?: RequestContext,
): Promise<Transaction> {
  return buildSpendTransaction(paidSpend(input), input, defaultSend, context);
}

export function createRingExitSubmission(
  input: RingExitTransactionParams & RingDirectRail,
  context?: RequestContext,
): Promise<RingTransactionSubmission> {
  return createSpendSubmission(paidSpend(input), input, defaultSend, context);
}

/** Default UTXOs of the sender enter the ring as its own ring notes. */
export async function buildRingEntryTransaction(
  input: RingEntryTransactionParams & RingOwnerRail,
  context?: RequestContext,
): Promise<Transaction> {
  return buildSpendTransaction(input, input, entryStrategy, context);
}

export function createRingEntrySubmission(
  input: RingEntryTransactionParams & RingDirectRail,
  context?: RequestContext,
): Promise<RingTransactionSubmission> {
  return createSpendSubmission(input, input, entryStrategy, context);
}

/**
 * Value leaves the ring to a plain Solana account. The recipient, the amount and the asset are
 * public, and the audit proof still covers the exit.
 */
export async function buildRingWithdrawalTransaction(
  input: RingWithdrawalTransactionParams & RingOwnerRail,
  context?: RequestContext,
): Promise<Transaction> {
  return buildSpendTransaction(input, input, withdrawalStrategy, context);
}

export function createRingWithdrawalSubmission(
  input: RingWithdrawalTransactionParams & RingDirectRail,
  context?: RequestContext,
): Promise<RingTransactionSubmission> {
  return createSpendSubmission(input, input, withdrawalStrategy, context);
}

/** Ring RPC reads the paths and records, proves and sends. */
export function prepareRingTransferRelay<R extends RingRelayRail>(
  input:
    | RingRelaySpendParams<RingTransferTransactionParams, R>
    | RingRelaySpendParams<RingMultiTransferParams, R>,
  context?: RequestContext,
): Promise<PreparedSpendRelay> {
  const params = paidSpend(input);
  return relayRingSpend({ params, strategy: ringSend(params) }, context);
}

export function prepareRingExitRelay<R extends RingRelayRail>(
  input: RingRelaySpendParams<RingExitTransactionParams, R>,
  context?: RequestContext,
): Promise<PreparedSpendRelay> {
  const params = paidSpend(input);
  return relayRingSpend({ params, strategy: defaultSend(params) }, context);
}

/** Anonymous rings alone, the in-proof owner of a confidential relay refuses every default-ring note. */
export function prepareRingEntryRelay(
  input: RingRelaySpendParams<RingEntryTransactionParams, RingAnonymousRelayRail>,
  context?: RequestContext,
): Promise<PreparedRingRelay> {
  return relayRingSpend({ params: input, strategy: entryStrategy(input) }, context);
}

export function prepareRingWithdrawalRelay<R extends RingRelayRail>(
  input: RingRelaySpendParams<RingWithdrawalTransactionParams, R>,
  context?: RequestContext,
): Promise<PreparedSpendRelay> {
  return relayRingSpend(
    {
      params: input,
      strategy: withdrawalStrategy({ ...input, feePayer: relaySpend(input).quote.feePayer }),
    },
    context,
  );
}

/** UTXOs the mode admits over the fewest input trees, largest first, at most `maxInputs`. */
export function selectRingInputs(
  input: Readonly<{
    wallet: Wallet;
    ringProgramId: Address;
    asset: Address;
    amount: bigint;
    inputs: "ring" | "ring-or-default" | "default" | "entry";
    maxInputs: number;
  }>,
): readonly WalletUtxo[] {
  const { wallet, ringProgramId, asset, amount, inputs } = input;
  // Zero selects a UTXO whose whole change would cross the ring boundary.
  if (amount <= 0n) throw new RingError("RING_ZERO_AMOUNT", { details: { asset } });
  const reserved = reservedUtxoKeys(wallet);
  const exposed = (entry: WalletUtxo): boolean =>
    entry.utxo.ringProgramId === ringProgramId && isZeroKeyNote(entry);
  const errors: SpendSelectionErrors = {
    ...ringSelectionErrors,
    insufficient: (shortfall) => {
      const exposedIfSpent = wallet
        .utxos()
        .filter(
          (entry) =>
            entry.utxo.asset === asset &&
            !entry.spent &&
            unreserved(reserved)(entry) &&
            exposed(entry),
        )
        .reduce((sum, entry) => sum + entry.utxo.amount, 0n);
      return exposedIfSpent === 0n
        ? ringSelectionErrors.insufficient(shortfall)
        : new RingError("RING_UNFUNDED_PAST_EXPOSURE", {
            details: {
              asset,
              requested: amount.toString(),
              exposedIfSpent: exposedIfSpent.toString(),
            },
          });
    },
  };
  const eligible = (entry: WalletUtxo): boolean => {
    if (!unreserved(reserved)(entry)) return false;
    if (exposed(entry)) return false;
    if (entry.utxo.ringProgramId === ringProgramId)
      return inputs === "ring" || inputs === "ring-or-default";
    return (
      inputs !== "ring" &&
      entry.utxo.ringProgramId === undefined &&
      entry.ringDataHash === undefined &&
      (inputs !== "entry" || carriesNoData([entry.dataHash]))
    );
  };
  return selectFunding({
    wallet,
    covers: [{ asset, amount }],
    policy: {
      eligible,
      ordering: "largestFirst",
      allowWideBalance: true,
      maxInputs: input.maxInputs,
      errors,
    },
  });
}

/** A plain build drops the viewing history and never takes the anonymous rail. */
function ownerRail(input: RingOwnerRail): RingOwnerRail {
  return input.inProofOwner === undefined ? {} : { inProofOwner: input.inProofOwner };
}

/** The strategy one operation stages its spend under, read again on every build. */
type SpendOperation<Spend, Resolved> = (spend: Spend) => RingSpendStrategy<Resolved>;

const ringSend = (spend: SendParams) => sendStrategy(spend, "ring");

const defaultSend = (spend: SendParams) => sendStrategy(spend, "default");

/** One build of an operation on the rail its caller names. */
function buildDirectSpend<Spend extends RingSpendParams, Resolved>(
  input: Readonly<{
    params: Spend;
    rail: RingDirectRail;
    operation: SpendOperation<Spend & SendRails, Resolved>;
    retry: RingSubmissionBuildState;
  }>,
  context: RequestContext | undefined,
): Promise<RingSubmissionAttempt> {
  const { params, rail, retry } = input;
  const railed = {
    ...params,
    ...(rail.anonymous === undefined ? {} : { anonymous: rail.anonymous }),
    ...(rail.spendPublic === undefined ? {} : { spendPublic: rail.spendPublic }),
  };
  return buildRingSpend({ params, rail, strategy: input.operation(railed), retry }, context);
}

async function buildSpendTransaction<Spend extends RingSpendParams, Resolved>(
  params: Spend,
  rail: RingOwnerRail,
  operation: SpendOperation<Spend & SendRails, Resolved>,
  context: RequestContext | undefined,
): Promise<Transaction> {
  const attempt = await buildDirectSpend(
    { params, rail: ownerRail(rail), operation, retry: {} },
    context,
  );
  return attempt.transaction;
}

function createSpendSubmission<Spend extends RingSpendParams, Resolved>(
  params: Spend,
  rail: RingDirectRail,
  operation: SpendOperation<Spend & SendRails, Resolved>,
  context: RequestContext | undefined,
): Promise<RingTransactionSubmission> {
  return RingTransactionSubmission.fromBuilder(
    {
      wallet: params.wallet,
      registry: params.client,
      build: (retry, buildContext) =>
        buildDirectSpend({ params, rail, operation, retry }, buildContext),
    },
    context,
  );
}

/** A spend whose recipients are its payments, `amount` their sum. */
type PaidSpend = SpendBase &
  Partial<Pick<RingTransferTransactionParams, "inputs">> &
  Readonly<{ payments: readonly RingTransferPayment[] }>;

/** A single recipient is one payment. */
function paidSpend<
  P extends
    | Readonly<{ recipient: RingTransferPayment["recipient"]; amount: bigint }>
    | Readonly<{ payments: readonly RingTransferPayment[] }>,
>(input: P): P & Readonly<{ amount: bigint; payments: readonly RingTransferPayment[] }> {
  const payments: readonly RingTransferPayment[] =
    "payments" in input ? input.payments : [{ recipient: input.recipient, amount: input.amount }];
  const amount = payments.reduce((sum, payment) => sum + payment.amount, 0n);
  return { ...input, amount, payments };
}

type SendRails = Readonly<{ anonymous?: AnonymousSpendOwner; spendPublic?: RingPublicSpend }>;

type SendParams = PaidSpend & SendRails;

function sendStrategy(
  params: SendParams,
  destination: "ring" | "default",
): RingSpendStrategy<readonly (ShieldedAddress | AnonymousRecipient)[]> {
  const { payments } = params;
  return {
    errorCode: "RING_BUILD_TRANSFER",
    selection: destination === "ring" ? (params.inputs ?? "ring") : "ring",
    payments: payments.length,
    resolve: (context) => {
      checkPayments(payments);
      return Promise.all(
        payments.map((paid) => resolveSendRecipient(params, paid, destination, context)),
      );
    },
    configure: ({ transfer, resolved: recipients, selected, asset, payment }) => {
      // Change of a default UTXO becomes ring bound.
      const defaultFunding = selected
        .filter((entry) => entry.utxo.ringProgramId === undefined)
        .reduce((sum, entry) => sum + entry.utxo.amount, 0n);
      const crossing =
        defaultFunding === 0n
          ? ""
          : `, moves ${String(defaultFunding)} ${assetLabel(asset)} of default UTXOs into the ring`;
      if (recipients.length > 1) {
        const paid = recipients.map((resolved, index) => {
          const { amount } = payments[index] as RingTransferPayment;
          if (resolved instanceof ShieldedAddress) {
            transfer.send(resolved, asset, amount);
            return { recipient: resolved, amount };
          }
          payment(resolved.to, asset, amount, resolved.reply);
          return { recipient: resolved.to, amount };
        });
        return {
          intent: {
            kind: "ringMultiTransfer",
            ringProgramId: params.ringProgramId,
            asset,
            payments: paid,
            defaultFunding,
          },
          summary: `ring transfer of ${String(params.amount)} ${assetLabel(asset)} in ring ${params.ringProgramId} to ${String(paid.length)} recipients${crossing}`,
        };
      }
      const [resolved] = recipients;
      if (resolved === undefined) throw new RingError("RING_ZERO_AMOUNT", { details: { asset } });
      if (!(resolved instanceof ShieldedAddress)) {
        payment(resolved.to, asset, params.amount, resolved.reply);
        return {
          intent: {
            kind: "ringAnonymousTransfer",
            ringProgramId: params.ringProgramId,
            asset,
            amount: params.amount,
            recipient: resolved.to,
            defaultFunding,
          },
          summary: `anonymous ring payment of ${String(params.amount)} ${assetLabel(asset)} in ring ${params.ringProgramId}${crossing}`,
        };
      }
      if (destination === "ring") transfer.send(resolved, asset, params.amount);
      else transfer.sendDefaultRing(resolved, asset, params.amount);
      const boundary =
        destination === "default" ? "exit" : defaultFunding > 0n ? "entry" : "transfer";
      return {
        intent: {
          kind: "ringTransfer",
          ringProgramId: params.ringProgramId,
          asset,
          amount: params.amount,
          recipient: resolved,
          boundary,
          defaultFunding,
        },
        summary: `ring ${boundary} of ${String(params.amount)} ${assetLabel(asset)} in ring ${params.ringProgramId} to a shielded address${crossing}`,
      };
    },
  };
}

function entryStrategy(params: SpendBase): RingSpendStrategy<undefined> {
  return {
    errorCode: "RING_BUILD_ENTRY",
    selection: "default",
    resolve: () => Promise.resolve(undefined),
    configure: ({ transfer, owner, asset, selected }) => {
      transfer.sendToRing(owner, asset, params.amount, params.ringProgramId);
      // The change stays bound to the ring, so the approval names every selected unit.
      const entered = selected.reduce((sum, entry) => sum + entry.utxo.amount, 0n);
      const change = entered - params.amount;
      return {
        intent: {
          kind: "ringEntry",
          ringProgramId: params.ringProgramId,
          asset,
          amount: entered,
        },
        summary: `ring entry of ${String(entered)} ${assetLabel(asset)} into ring ${params.ringProgramId}${change === 0n ? "" : `, ${String(change)} ${assetLabel(asset)} of it as change`}`,
      };
    },
  };
}

type WithdrawalParams = SpendBase &
  Pick<RingWithdrawalTransactionParams, "recipient" | "splTokenProgram" | "feePayer">;

function withdrawalStrategy(params: WithdrawalParams): RingSpendStrategy<
  Readonly<{
    settlement: Awaited<ReturnType<typeof resolveWithdrawalSettlement>>;
    setupInstructions: readonly Instruction[];
  }>
> {
  return {
    errorCode: "RING_BUILD_WITHDRAWAL",
    selection: "ring",
    resolve: async () => {
      const asset = params.asset ?? SOL_MINT;
      const [settlement, setupInstructions] = await Promise.all([
        resolveWithdrawalSettlement(params.recipient, asset, params.splTokenProgram),
        withdrawalSetupInstructions({
          payer: params.feePayer,
          recipient: params.recipient,
          asset,
          ...(params.splTokenProgram === undefined
            ? {}
            : { splTokenProgram: params.splTokenProgram }),
        }),
      ]);
      return { settlement, setupInstructions };
    },
    configure: ({ transfer, resolved, asset }) => {
      transfer.withdraw(asset, params.amount, resolved.settlement.target);
      return {
        intent: {
          kind: "ringWithdrawal",
          ringProgramId: params.ringProgramId,
          asset,
          amount: params.amount,
          recipient: withdrawalIntentRecipient(resolved.settlement.target),
        },
        summary: `public withdrawal of ${String(params.amount)} ${assetLabel(asset)} from ring ${params.ringProgramId} to ${params.recipient}`,
        withdrawal: resolved.settlement.accounts,
        withdrawalOwner: params.recipient,
        setupInstructions: resolved.setupInstructions,
        ...(resolved.setupInstructions.length === 0
          ? {}
          : { tokenAccountSetup: [{ owner: params.recipient, mint: asset }] }),
      };
    },
  };
}

interface RingSpendPlan {
  readonly intent: RingTransactionIntent;
  readonly summary: string;
  readonly withdrawal?: TransactWithdrawal;
  /** The wallet whose token account receives the withdrawal. */
  readonly withdrawalOwner?: Address;
  readonly setupInstructions?: readonly Instruction[];
  /** The accounts Ring RPC creates in place of `setupInstructions`. */
  readonly tokenAccountSetup?: readonly TokenAccountSetup[];
}

interface RingSpendStrategy<R> {
  readonly errorCode: "RING_BUILD_ENTRY" | "RING_BUILD_TRANSFER" | "RING_BUILD_WITHDRAWAL";
  readonly selection: "ring" | "ring-or-default" | "default";
  /** Outputs the spend pays beside its change, one when unset. */
  readonly payments?: number;
  resolve(context: RequestContext | undefined): Promise<R>;
  configure(
    input: Readonly<{
      transfer: ConfidentialTransfer;
      resolved: R;
      selected: readonly WalletUtxo[];
      asset: Address;
      owner: ShieldedAddress;
      /** Pays a ring address on the anonymous rail. */
      payment: (
        to: RingAddress,
        asset: Address,
        amount: bigint,
        reply: ReplyMetadata | null,
      ) => void;
    }>,
  ): RingSpendPlan;
}

type SpendBase = Omit<RingSpendParams, "feePayer" | "cosigners" | "priorityFeeLamports">;

/** The spend selected, reserved and configured, before its rail proves or relays it. */
interface StagedSpend {
  readonly selected: readonly WalletUtxo[];
  readonly rail: CustomRingTransfer;
  readonly plan: RingSpendPlan;
  readonly planned: Bytes32;
  readonly anonymousPayments: readonly AnonymousPayment[];
  readonly owner: ShieldedAddress;
}

/** One build attempt, the reservation `retry` holds survives every rebuild of a submission. */
async function buildRingSpend<R>(
  input: Readonly<{
    params: RingSpendParams;
    rail: RingDirectRail;
    strategy: RingSpendStrategy<R>;
    retry: RingSubmissionBuildState;
  }>,
  context: RequestContext | undefined,
): Promise<RingSubmissionAttempt> {
  const { params, rail, strategy, retry } = input;
  try {
    const staged = await stageRingSpend(
      {
        params,
        feePayer: params.feePayer,
        strategy,
        retry,
        route:
          rail.spendPublic !== undefined
            ? "public"
            : rail.anonymous === undefined
              ? "confidential"
              : "anonymous",
        ownerInProof: rail.inProofOwner !== undefined,
        ...(params.cosigners === undefined
          ? {}
          : { coSigners: params.cosigners.map((cosigner) => signerAddress(cosigner)) }),
      },
      context,
    );
    const { plan, owner } = staged;
    const approval: RingTransferApproval = {
      transaction: {
        ...(params.priorityFeeLamports === undefined
          ? {}
          : { priorityFeeLamports: params.priorityFeeLamports }),
        ...(plan.setupInstructions === undefined
          ? {}
          : { setupInstructions: plan.setupInstructions }),
      },
      approve: async ({ prepared }) => {
        await approveSpend({ params, feePayer: params.feePayer, plan, owner, prepared });
      },
    };
    const proven = await proveDirect({ staged, client: params.client, rail, approval }, context);
    return await provenRingAttempt(
      {
        proven,
        feePayer: params.feePayer,
        chain: params.client,
        transaction: approval.transaction,
        intentHash: staged.planned,
      },
      context,
    );
  } catch (cause) {
    if (retry.reservation !== undefined) params.wallet._releaseReservation(retry.reservation.id);
    throw wrapRingError(strategy.errorCode, cause);
  }
}

/** The relayed terminal, the selected notes stay leased for `DEFAULT_RESERVATION_TTL_MS`. */
async function relayRingSpend<S, R extends RingRelayRail>(
  input: Readonly<{
    params: SpendBase & R;
    strategy: RingSpendStrategy<S>;
  }>,
  context: RequestContext | undefined,
): Promise<PreparedSpendRelay> {
  const { params, strategy } = input;
  const spend = relaySpend(params);
  const { anonymous } = params;
  const retry: RingSubmissionBuildState = {};
  try {
    const staged = await stageRingSpend(
      {
        params,
        feePayer: spend.quote.feePayer,
        strategy,
        retry,
        route: anonymous === undefined ? "confidential" : "anonymous",
        ownerInProof: anonymous === undefined,
      },
      context,
    );
    const { plan, owner } = staged;
    const relayInput = {
      quote: spend.quote,
      nullifierKey: spend.nullifierKey,
      notes: staged.selected,
      ...(plan.tokenAccountSetup === undefined
        ? {}
        : { tokenAccountSetup: plan.tokenAccountSetup }),
      ...(spend.sealRow === undefined ? {} : { sealRow: spend.sealRow }),
    };
    const approval: RingTransferApproval = {
      transaction: {},
      approve: ({ prepared }) =>
        approveSpend({ params, feePayer: spend.quote.feePayer, plan, owner, prepared }),
    };
    // `R` holds `anonymous` exactly when the spend returns a viewing history.
    if (anonymous === undefined)
      return Object.freeze({
        relay: await staged.rail
          .withInProofOwner(spend.signer)
          .prepareRelayApproved(params.client, relayInput, approval, context),
      }) as PreparedSpendRelay;
    return (await anonymousTransfer(staged, anonymous).prepareRelayApproved(
      params.client,
      relayInput,
      approval,
      context,
    )) as PreparedSpendRelay;
  } catch (cause) {
    if (retry.reservation !== undefined) params.wallet._releaseReservation(retry.reservation.id);
    throw wrapRingError(strategy.errorCode, cause);
  }
}

function relaySpend(rail: RingRelayRail): RingRelaySpend {
  return rail.anonymous === undefined ? rail.confidential : rail.anonymous;
}

function proveDirect(
  input: Readonly<{
    staged: StagedSpend;
    client: RingTransferClient;
    rail: RingDirectRail;
    approval: RingTransferApproval;
  }>,
  context: RequestContext | undefined,
): Promise<ProvenRingTransfer> {
  const { staged, rail, approval } = input;
  if (rail.spendPublic !== undefined) {
    const paying = staged.anonymousPayments.reduce(
      (named, payment) => named.withPayment(payment),
      staged.rail.spendPublic(rail.spendPublic.wallet),
    );
    return paying.proveApproved(input.client, approval, context);
  }
  if (rail.anonymous !== undefined)
    return anonymousTransfer(staged, rail.anonymous).proveApproved(rail.client, approval, context);
  const owner =
    rail.inProofOwner === undefined ? staged.rail : staged.rail.withInProofOwner(rail.inProofOwner);
  return owner.proveApproved(input.client, approval, context);
}

function anonymousTransfer(staged: StagedSpend, owner: AnonymousSpendOwner): AnonymousTransfer {
  const paying = staged.anonymousPayments.reduce(
    (signed, payment) => signed.withPayment(payment),
    staged.rail.withAnonymousSigner(owner.signer),
  );
  const exposed =
    owner.ownerExposure === undefined ? paying : paying.withOwnerExposure(owner.ownerExposure);
  return owner.exitTokenOwner === undefined
    ? exposed
    : exposed.withExitTokenOwner(owner.exitTokenOwner);
}

/** Each route's refusal on a ring whose mode it does not serve, public owners serve either. */
const MODE_REFUSALS = Object.freeze({
  confidential: "RING_ANONYMOUS_SIGNER_REQUIRED",
  anonymous: "RING_NOT_ANONYMOUS",
} as const);

/** `retry` keeps the reservation for the caller to release. */
async function stageRingSpend<R>(
  input: Readonly<{
    params: SpendBase;
    feePayer: Address;
    strategy: RingSpendStrategy<R>;
    retry: RingSubmissionBuildState;
    route: keyof typeof MODE_REFUSALS | "public";
    /** Ring notes only, a default-ring input would publish the owner SPP hides. */
    ownerInProof: boolean;
    coSigners?: readonly Address[];
  }>,
  context: RequestContext | undefined,
): Promise<StagedSpend> {
  const { params, strategy, retry, route } = input;
  await initializePoseidon();
  const asset = params.asset ?? SOL_MINT;
  const owner = params.keys.address();
  const [resolved, config, outputTree] = await Promise.all([
    strategy.resolve(context),
    fetchRingPolicy(params.client, params.ringProgramId, context),
    resolveRingOutputTree(params.client, params.outputTree, context),
  ]);
  const anonymousRing = config.mode === "anonymous";
  if (route !== "public" && (route === "anonymous") !== anonymousRing)
    throw new RingError(MODE_REFUSALS[route], { details: { ringProgramId: params.ringProgramId } });
  if (retry.entries !== undefined) checkRetainedEntries(params.wallet, retry.entries);
  const pool = {
    wallet: params.wallet,
    ringProgramId: params.ringProgramId,
    asset,
    amount: params.amount,
  };
  const selected =
    retry.entries ??
    (route !== "public" && anonymousRing
      ? selectAnonymousInputs(pool, strategy.selection, strategy.payments)
      : selectRingInputs({
          ...pool,
          inputs:
            input.ownerInProof && strategy.selection === "ring-or-default"
              ? "ring"
              : strategy.selection,
          maxInputs: MAX_TRANSACT_INPUTS,
        }));
  retry.reservation = retry.reservation ?? reserveEntries(params.wallet, selected, retry.lifetime);
  retry.entries = selected;
  const transfer = ringSpendTransfer({
    owner,
    inputs: await ringProofInputs(selected, () => owner, params.client, context),
    feePayer: input.feePayer,
  }).withOutputTreeId(outputTree.treeId);
  const anonymousPayments: AnonymousPayment[] = [];
  const plan = strategy.configure({
    transfer,
    resolved,
    selected,
    asset,
    owner,
    payment: (to, paidAsset, amount, reply) => {
      if (!anonymousRing)
        throw new RingError("RING_NOT_ANONYMOUS", {
          details: { ringProgramId: params.ringProgramId },
        });
      anonymousPayments.push({ to, asset: paidAsset, amount, reply });
    },
  });
  const planned = ringIntentHash(plan.intent);
  if (retry.intent !== undefined && !equalBytes(retry.intent, planned))
    throw ringIntentMismatch("retryIntent");
  retry.intent = planned;
  const ownerRail = CustomRingTransfer.new({
    ringProgramId: params.ringProgramId,
    keys: params.keys,
    transfer,
    assets: params.wallet.registry,
    ...(input.coSigners === undefined ? {} : { coSigners: input.coSigners }),
    listSalts: spendListSalts(params),
    ...(params.ringSalt === undefined ? {} : { ringSalt: params.ringSalt }),
  });
  return Object.freeze({
    selected,
    rail:
      plan.withdrawal === undefined
        ? ownerRail
        : ownerRail.withWithdrawalAccounts(plan.withdrawal, plan.withdrawalOwner),
    plan,
    planned,
    anonymousPayments: Object.freeze(anonymousPayments),
    owner,
  });
}

async function approveSpend(
  input: Readonly<{
    params: SpendBase;
    feePayer: Address;
    plan: RingSpendPlan;
    owner: ShieldedAddress;
    prepared: PreparedTransfer;
  }>,
): Promise<void> {
  const { params, plan } = input;
  const approved = await (params.approve ?? approveRingUnattended)({
    solanaPublicKey: ownerSolanaAccount(input.owner, input.feePayer),
    intent: plan.intent,
    summary: plan.summary,
  });
  checkRingIntentApproval(approved, plan.intent, ringIntentMismatch);
  checkRingPreparedTransfer(input.prepared, plan.intent, ringIntentMismatch);
}

/** The hidden owner rail refuses a mix of ring and default-ring UTXOs, ring UTXOs alone fund it first. */
export function selectAnonymousInputs(
  pool: Omit<Parameters<typeof selectRingInputs>[0], "inputs" | "maxInputs">,
  selection: "ring" | "ring-or-default" | "default",
  payments = 1,
): readonly WalletUtxo[] {
  const select = (inputs: "ring" | "entry") =>
    selectRingInputs({ ...pool, inputs, maxInputs: anonymousInputCap(inputs, payments) });
  if (selection !== "ring-or-default") return select(selection === "ring" ? "ring" : "entry");
  try {
    return select("ring");
  } catch (ringAlone) {
    if (!unfunded(ringAlone)) throw ringAlone;
    try {
      return select("entry");
    } catch (defaultAlone) {
      if (
        !unfunded(defaultAlone) ||
        (balanceShortfall(ringAlone) && !balanceShortfall(defaultAlone))
      )
        throw defaultAlone;
      throw ringAlone;
    }
  }
}

function spendListSalts(params: SpendBase): readonly RingListSalt[] {
  const recipients =
    "payments" in params
      ? (params as PaidSpend).payments.map((payment) => payment.recipient)
      : "recipient" in params
        ? [params.recipient]
        : [];
  return [
    ...(params.listSalts ?? []),
    ...recipients.flatMap((recipient) => {
      const address =
        recipient instanceof RingPaymentRequest
          ? recipient.address
          : recipient instanceof RingAddress
            ? recipient
            : undefined;
      return address === undefined || isZero(address.listSalt)
        ? []
        : [
            {
              owner: address.recipient.signingPublicKey.ownerProofInputHash(),
              salt: address.listSalt,
            },
          ];
    }),
  ];
}

function unfunded(cause: unknown): cause is RingError {
  return (
    cause instanceof RingError &&
    (balanceShortfall(cause) ||
      cause.code === "RING_TOO_MANY_INPUTS" ||
      cause.code === "RING_TOO_MANY_INPUT_TREES")
  );
}

function balanceShortfall(cause: RingError): boolean {
  return cause.code === "RING_INSUFFICIENT_BALANCE" || cause.code === "RING_UNFUNDED_PAST_EXPOSURE";
}

/** @internal Padded inputs are published, the widest shape an anonymous key serves for `payments` and the sender change bounds a spend. */
export function anonymousInputCap(selection: "ring" | "entry", payments = 1): number {
  // Only the narrow sixteen output key holds a frame past eight outputs.
  const keyed = (inputs: number) =>
    [true, false].some(
      (wide) =>
        frameKeyShape({
          rail: "anonymous",
          inputs,
          outputs: payments + 1,
          spendsDefaultRing: selection === "entry",
          wide,
        }) !== undefined,
    );
  let inputs = MAX_TRANSACT_INPUTS;
  while (inputs > 0 && !keyed(inputs)) inputs -= 1;
  return inputs;
}

type AnonymousRecipient = Readonly<{
  to: RingAddress;
  reply: ReplyMetadata | null;
}>;

/** One to `MAX_RING_TRANSFER_PAYMENTS` payments, each one nonzero. */
function checkPayments(payments: readonly RingTransferPayment[]): void {
  if (payments.length > MAX_RING_TRANSFER_PAYMENTS)
    throw new RingError("RING_TRANSFER_TOO_WIDE", {
      details: { payments: payments.length, limit: MAX_RING_TRANSFER_PAYMENTS },
    });
  if (payments.length > 1 && payments.some((payment) => payment.amount <= 0n))
    throw new RingError("RING_ZERO_AMOUNT", { details: { reason: "payment" } });
}

/** A ring address or request pays on the anonymous rail, any other recipient resolves to its shielded address. */
async function resolveSendRecipient(
  params: SendParams,
  paid: RingTransferPayment,
  destination: "ring" | "default",
  context: RequestContext | undefined,
): Promise<ShieldedAddress | AnonymousRecipient> {
  const { recipient } = paid;
  if (recipient instanceof RingAddress || recipient instanceof RingPaymentRequest) {
    const to = recipient instanceof RingAddress ? recipient : recipient.address;
    if (destination !== "ring" || to.ringProgramId !== params.ringProgramId)
      throw new RingError("RING_FOREIGN_RING", {
        details: { address: to.ringProgramId, ring: params.ringProgramId },
      });
    checkRequestTerms(params, paid.amount, recipient);
    return Object.freeze({ to, reply: replyChoice(params) });
  }
  return resolveShieldedRecipient(
    { rpc: params.client, recipient },
    (unregistered) =>
      new RingError("RING_BUILD_TRANSFER", {
        details: { reason: "recipient not registered", recipient: unregistered },
      }),
    context,
  );
}

/** The caller names the choice on every anonymous payment. */
function replyChoice(params: SendParams): ReplyMetadata | null {
  const reply = params.anonymous === undefined ? params.spendPublic?.reply : params.anonymous.reply;
  if (reply === undefined)
    throw new RingError("RING_REPLY_INVALID", { details: { reason: "unnamed" } });
  return reply;
}

/** A request names its amount and mint. */
function checkRequestTerms(
  params: SendParams,
  amount: bigint,
  recipient: RingAddress | RingPaymentRequest,
): void {
  if (recipient instanceof RingAddress) return;
  const asset = params.asset ?? SOL_MINT;
  if (recipient.amount !== undefined && recipient.amount !== amount)
    throw ringIntentMismatch("amount");
  if (recipient.mint !== undefined && recipient.mint !== asset) throw ringIntentMismatch("asset");
}

export function assetLabel(asset: Address): string {
  return asset === SOL_MINT ? "SOL" : asset;
}

export type RingTransfersInput = Readonly<{
  client: RingTransferClient;
  ringProgramId: Address;
  assets: CustomRingTransferInput["assets"];
  listSalts?: readonly RingListSalt[];
}>;

type RingPaymentRail = Readonly<{
  proveApproved(
    client: RingTransferClient,
    approval: RingTransferApproval,
    context?: RequestContext,
  ): Promise<ProvenRingTransfer>;
}>;

/** `take` refuses a transfer another line wrote into ahead of any write, the request's transfer serves one call. */
function ringTransfers(
  input: RingTransfersInput,
  rail: (
    take: () => CustomRingTransfer,
    request: Parameters<RingTransfers["proveRingTransfer"]>[0],
    ring: Omit<RingTransfersInput, "client">,
    signer: AnonymousSpendSigner | undefined,
  ) => RingPaymentRail,
): RingTransfers {
  const { client, ...ring } = input;
  return {
    ringProgramId: ring.ringProgramId,
    proveRingTransfer: async (request, context) => {
      const { treeId } = await resolveRingOutputTree(client, undefined, context);
      const take = (): CustomRingTransfer =>
        CustomRingTransfer.new({
          ...ring,
          keys: request.keys,
          transfer: request.transfer,
        }).withOutputTree(treeId);
      let authorized: PreparedOwnerSignature | undefined;
      const approval: RingTransferApproval = {
        transaction: {},
        approve: async ({ proofInputs }) => {
          authorized = await request.authorize(proofInputs);
        },
      };
      const { owner } = request;
      const signer =
        owner === undefined
          ? undefined
          : {
              publicKey: () => owner,
              sign: async () => {
                if (authorized === undefined) throw new RingError("RING_OWNER_UNAUTHORIZED");
                return authorized.ownerSignature;
              },
            };
      const proven = await rail(take, request, ring, signer).proveApproved(
        client,
        approval,
        context,
      );
      return {
        instruction: await proven.instruction(),
        computeUnitLimit: provenComputeUnitLimit(proven),
      };
    },
  };
}

/** Rust `OwnerRingTransfers`, an `owner` signing inside the proof is refused on default-ring inputs. */
export function ownerRingTransfers(input: RingTransfersInput): RingTransfers {
  return ringTransfers(input, (take, { payment }, _ring, signer) => {
    const rail = take().withOutput(payment);
    return signer === undefined ? rail : rail.withInProofOwner(signer);
  });
}

/** Rust `AnonymousRingTransfers`, a payee missing from `payees` must be under the zero key. */
export function anonymousRingTransfers(
  input: RingTransfersInput & Readonly<{ payees: readonly RingAddress[] }>,
): RingTransfers {
  const { payees, ...rest } = input;
  return ringTransfers(rest, (take, { payment }, ring, signer) => {
    if (signer === undefined) throw new RingError("RING_ANONYMOUS_SIGNER_REQUIRED");
    const recipient = payment.recipient.toBytes();
    const to =
      payees.find(
        (address) =>
          address.ringProgramId === ring.ringProgramId &&
          equalBytes(address.recipient.toBytes(), recipient),
      ) ??
      new RingAddress({
        recipient: payment.recipient,
        ringProgramId: ring.ringProgramId,
        listSalt: new Uint8Array(32) as Bytes32,
      });
    return take()
      .withAnonymousSigner(signer)
      .withPayment({ to, asset: payment.asset, amount: payment.amount, reply: null });
  });
}
