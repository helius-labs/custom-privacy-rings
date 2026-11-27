/** One home for how a rail's transfer stages, proves and relays, every rail of `transfer-rails` runs through it. */
import { getBase64Decoder } from "@solana/kit";

import { equalBytes, ownerSignerAddresses, addressBytes } from "@heliuslabs/spr/internal";
import type { PreparedOwnerSignature, WalletKeys } from "@heliuslabs/spr/client";
import { InstructionTag } from "@heliuslabs/spr/interface";
import type {
  Address,
  Bytes16,
  Bytes32,
  RequestContext,
  TransactWithdrawal,
} from "@heliuslabs/spr/interface";
import { associatedTokenAddress, signerAddress } from "@heliuslabs/spr/internal";
import type { NullifierKey, ShieldedAddress, X25519PublicKey } from "@heliuslabs/spr/keypair";
import type {
  ExpectedSealed,
  RelayTransferTerms,
  TokenAccountCreate,
} from "../../rpc/relay-message.js";
import { randomBlinding, ShieldedPublicKey, ViewingKey } from "@heliuslabs/spr/keypair";
import type { AssetRegistry, ProofInputUtxo, WalletUtxo } from "@heliuslabs/spr/transaction";
import { SppProofInputs, type PreparedTransfer } from "@heliuslabs/spr/transaction";
import {
  encodeRelayProofInputs,
  relayLegs,
  type RelayProofInputs,
  type RelayLegAccounts,
} from "../../protocol/ring-rpc/relay-proof-inputs.js";
import type {
  RelayIntent,
  RelayQuote,
  TokenAccountSetup,
} from "../../protocol/ring-rpc/ring-rpc-types.js";
import { UnsignedRelay } from "../../rpc/relay.js";

import {
  anonymousExitTokenOwner,
  checkAnonymousSigner,
  checkHiddenParties,
  checkHiddenOwnerSpends,
  checkEntryInputs,
  paymentTag,
  type AnonymousPayment,
  type OwnerExposure,
  type AnonymousSpendSigner,
} from "./anonymous.js";
import type { RingPolicy } from "../../protocol/codec/ring-codecs.js";
import { fetchRingPolicy, requireActiveRing } from "../../protocol/membership/config.js";
import { RingError, RingProgramError } from "../../protocol/error.js";
import { chargedOutflowsOf, relayListSalts } from "../../proof/policy-proof.js";
import { transferApprovalOf } from "../../proof/transfer-proof.js";
import { isDefaultRingNote, spentNotes } from "../../proof/audit-proof.js";
import { anonymousPartyIdentities } from "../../protocol/membership/parties.js";
import {
  proveStagedTransfer,
  sealRelayedTransfer,
  sealedFunderTerms,
  sealedTransferTerms,
  type SealedTransfer,
  type CoSignerForm,
  type ProvenRingTransfer,
  type RailSpend,
  type RingProvingClient,
  type StagedTransfer,
  type StagedTransferInput,
  type TransactDeposit,
} from "../workflow/pipeline.js";
import {
  checkProgramCommitment,
  ProgramPlan,
  type ProgramCommitment,
} from "../workflow/program-commitment.js";
import type { SealRow } from "../workflow/transfer-plan.js";
import { isSmallOrder } from "../../crypto/keys/x25519.js";
import type { TransferTags } from "../../state/viewing-history.js";
import { RingOutputRail, type RingDelivery } from "./output-rail.js";
import { AnonymousPlan } from "../../protocol/transaction/anonymous-plan.js";
import { current, type TransferLine } from "./transfer-line.js";
import type {
  AnonymousTransferClient,
  CustomRingTransferInput,
  DepositRelayFrame,
  DepositRelayInput,
  PublicOwnerTransferClient,
  ReadCache,
  RingRelayInput,
  RingTransferApproval,
} from "./transfer-rails.js";
import { checkCoOwnerKeys, checkZeroKeyInputs, ringSpendAuthority } from "./ring-spend.js";
import { type CoOwner } from "../../protocol/ring-rpc/relay-proof-inputs.js";
const base64 = getBase64Decoder();

export type Owned = TransferLine<CustomRingTransferInput>;

async function preparedOwnerSignature(
  signer: AnonymousSpendSigner,
  owner: Bytes32,
  proofInputs: SppProofInputs,
  context: RequestContext | undefined,
): Promise<PreparedOwnerSignature> {
  const ownerSignature = await signer.sign(proofInputs.messageHash(), context);
  if (!(ownerSignature instanceof Uint8Array) || ownerSignature.length !== 64)
    throw new RingError("RING_INVALID_LENGTH", {
      details: {
        name: "ownerSignature",
        expected: 64,
        actual: ownerSignature instanceof Uint8Array ? ownerSignature.length : -1,
      },
    });
  return {
    ownerPublicKey: owner,
    ownerSignature,
  };
}

/** SPP's anonymous rail publishes the owner of a default-ring note, Rust `RingError::InProofDefaultRingInput`. */
function checkRingNotes(proofInputs: SppProofInputs): void {
  const index = proofInputs.inputUtxos.findIndex(isDefaultRingNote);
  if (index >= 0) throw new RingError("RING_IN_PROOF_DEFAULT_RING_INPUT", { details: { index } });
}

/** Padding alone, an address input still needs its owner to sign. */
function spendsNothing(proofInputs: SppProofInputs): boolean {
  return proofInputs.inputUtxos.every((spent) => spent.isDummy() || spent.isCompact());
}

/** The signer must hold the sender's signing key. */
function inProofOwnerKey(
  signer: AnonymousSpendSigner | undefined,
  sender: ShieldedAddress,
): Bytes32 | undefined {
  if (signer === undefined) return undefined;
  const signing = signer.publicKey();
  if (!(signing instanceof ShieldedPublicKey) || !signing.equals(sender.signingPublicKey))
    throw new RingError("RING_IN_PROOF_OWNER_MISMATCH");
  return signing.ed25519();
}

/** One spend's payments with the wallet's tag selection, Rust `TaggedPayments`. */
type TaggedPayments = Readonly<{
  prepared: PreparedTransfer;
  sender: ShieldedAddress;
  payments: readonly AnonymousPayment[];
  tags: TransferTags | undefined;
}>;

/** Without the wallet's sender tag change takes random bytes, the note the wallet spends finds it. */
function taggedPlan(tagged: TaggedPayments): AnonymousPlan {
  const { prepared, sender, payments, tags } = tagged;
  // 1. An entry spends the sender's default-ring notes and no ring note of its own, a core sync
  // that reads their spend first leaves only the sender tag to find the change.
  const own = spentNotes(prepared.inputs).filter((input) =>
    input.utxo.owner.equals(sender.signingPublicKey),
  );
  const entry = own.length > 0 && own.every(isDefaultRingNote);
  // 2. Change takes the sender tag only under the key the tags derive from, a deposit's
  // fresh sender holds another key and its change a random tag.
  const senderTags = tags?.viewingPublicKey.equals(sender.viewingPublicKey) ? tags : undefined;
  if (senderTags === undefined && entry) throw new RingError("RING_SENDER_TAG_REQUIRED");
  const plan =
    senderTags === undefined
      ? AnonymousPlan.create()
      : AnonymousPlan.create().withSenderTag(senderTags.sender);
  // 3. The send call alone attaches a reply, no default path does.
  return payments.reduce((planned, payment) => {
    const paid = planned.withPayment({
      recipient: payment.to.recipient.viewingPublicKey,
      reply: payment.reply,
    });
    const tag = paymentTag(payment, tags);
    return tag === undefined ? paid : paid.withTag(tag);
  }, plan);
}

export type ConfidentialRail = Readonly<{
  kind: "confidential";
  inProofOwner: AnonymousSpendSigner | undefined;
}>;

export type AnonymousRail = Readonly<{
  kind: "anonymous";
  signer: AnonymousSpendSigner;
  payments: readonly AnonymousPayment[];
  tags: TransferTags | undefined;
  exitTokenOwner: Address | undefined;
  readCache: ReadCache | undefined;
  exposure: OwnerExposure;
}>;

export type PublicOwnersRail = Readonly<{
  kind: "publicOwners";
  wallet: Address;
  /** Set when the rail it came from signs inside the proof, staging refuses it. */
  inProofOwner: boolean;
  payments: readonly AnonymousPayment[];
  tags: TransferTags | undefined;
}>;

type OwnerRail = ConfidentialRail | AnonymousRail | PublicOwnersRail;

export type RailTransfer<Rail extends OwnerRail = OwnerRail> = Readonly<{
  owner: Owned;
  withdrawal: TransactWithdrawal | undefined;
  coOwners: readonly CoOwner[];
  rail: Rail;
}>;

/** A transfer past its outputs' sealing, whoever holds it destroys `staged.rail.txViewingKey`. */
type StagedRail = Readonly<{
  config: RingPolicy;
  prepared: PreparedTransfer;
  staged: Omit<StagedTransfer, "spend">;
  spend: Omit<RailSpend, "prove">;
  /** The key signing inside the SPP proof, the hidden owner on the anonymous rail. */
  inProofOwner: Bytes32 | undefined;
  approvalBlinder: Bytes32 | undefined;
}>;

/** A secret a staged transfer holds is destroyed on every exit, here once staging throws. */
export async function stageTransfer(
  transfer: RailTransfer,
  client: Pick<RingProvingClient, "getAccount">,
  context: RequestContext | undefined,
  planSalt?: Bytes16,
): Promise<StagedRail> {
  const { owner, withdrawal, coOwners, rail } = transfer;
  const { ringProgramId, keys, program } = owner;
  const sender = keys.address();
  refuseUnread(transfer, sender);
  const config = await activeConfig(client, ringProgramId, context);
  const anonymous = config.mode === "anonymous";
  const salt = planSalt ?? program?.salt;
  checkRingMode(rail, { anonymous, salt, ringProgramId });
  const inProofOwner =
    rail.kind === "confidential" ? inProofOwnerKey(rail.inProofOwner, sender) : undefined;
  const prepared = current(owner).transfer.prepare();
  checkWithdrawalAccounts(prepared, withdrawal);
  if (rail.kind !== "anonymous") checkDepositAccounts(prepared, owner.deposits);
  const hidden =
    rail.kind === "anonymous"
      ? await hiddenOwner({ rail, deposits: owner.deposits, client, sender, prepared }, context)
      : undefined;
  const payments = rail.kind === "confidential" ? [] : rail.payments;
  const tags = rail.kind === "confidential" ? undefined : rail.tags;
  if (anonymous && payments.some((payment) => isSmallOrder(payment.to.recipient.viewingPublicKey)))
    throw new RingError("RING_RECIPIENT_KEY_NOT_CONTRIBUTORY");
  const spent = prepared.inputs.find((input) => input.utxo.amount > 0n)?.utxo.asset;
  return sealOutputs(
    {
      key: await transactionKey(keys, sender, prepared, context),
      prepared,
      delivery: () =>
        anonymous
          ? { kind: "anonymous", plan: taggedPlan({ prepared, sender, payments, tags }) }
          : { kind: "delivered" },
      payments,
      salt,
      sender,
      assets: owner.assets,
      auditor: config.auditorPublicKey,
      instructionDiscriminator: InstructionTag.ringTransact,
      zeroChangeAsset:
        rail.kind === "publicOwners" && (planSalt ?? program) !== undefined ? spent : undefined,
    },
    (outputs, encrypted) => {
      const proofInputs =
        rail.kind === "anonymous" && rail.readCache !== undefined
          ? withReadCacheSlots(encrypted, rail.readCache)
          : encrypted;
      checkSealedInputs(rail, { proofInputs, sender, inProofOwner, hidden });
      const signing = inProofOwner ?? hidden?.owner;
      // An in-proof owner may sit beside signing co-owners, the run names them alone.
      const ownerSigners = ownerSignerAddresses(
        proofInputs.inputUtxos,
        proofInputs.outputs,
        signing,
      );
      if (
        rail.kind === "confidential" &&
        !spendsNothing(proofInputs) &&
        signing === undefined &&
        ownerSigners.length === 0
      )
        throw new RingError("RING_NO_SIGNING_OWNER");
      checkCoOwnerKeys(proofInputs, sender, coOwners);
      return {
        config,
        prepared,
        inProofOwner: signing,
        approvalBlinder: attemptBlinder(owner),
        spend: stagedSpend(rail, { anonymous, inProofOwner, hidden }),
        staged: {
          proofInputs,
          rail: outputs,
          sender,
          ownerSigners,
          ...(rail.kind === "anonymous" ? {} : stagedDeposits(owner)),
          ...(withdrawal === undefined ? {} : { withdrawal }),
          ...(rail.kind !== "confidential" || owner.withdrawalOwner === undefined
            ? {}
            : { withdrawalOwner: owner.withdrawalOwner }),
          ...(owner.extensions === undefined ? {} : { extensions: owner.extensions }),
          ...(planSalt === undefined ? committedStage(program, proofInputs) : {}),
        },
      };
    },
  );
}

/** Refusals no read of the ring decides. */
function refuseUnread(transfer: RailTransfer, sender: ShieldedAddress): void {
  const { owner, rail } = transfer;
  if (rail.kind === "anonymous" && owner.program !== undefined)
    throw new RingError("RING_PROGRAM_RAIL_UNSUPPORTED");
  if (rail.kind !== "publicOwners") return;
  if (sender.solanaAddress() !== rail.wallet)
    throw new RingError("RING_PUBLIC_SPEND_WALLET_MISMATCH", {
      details: { named: rail.wallet, sender: sender.solanaAddress() },
    });
  if (rail.inProofOwner) throw new RingError("RING_PUBLIC_SPEND_IN_PROOF_OWNER");
}

/** Public owners settle on either ring mode. */
function checkRingMode(
  rail: OwnerRail,
  ring: Readonly<{ anonymous: boolean; salt: Bytes16 | undefined; ringProgramId: Address }>,
): void {
  const { ringProgramId } = ring;
  if (rail.kind === "anonymous" && !ring.anonymous)
    throw new RingError("RING_NOT_ANONYMOUS", { details: { ringProgramId } });
  if (rail.kind !== "confidential") return;
  if (ring.anonymous)
    throw new RingError("RING_ANONYMOUS_SIGNER_REQUIRED", { details: { ringProgramId } });
  // A program's proof holds hashes only a transaction signer's rail reproduces.
  if (ring.salt !== undefined && rail.inProofOwner !== undefined)
    throw new RingError("RING_PROGRAM_RAIL_UNSUPPORTED");
}

type HiddenOwner = Readonly<{
  owner: Bytes32;
  /** Unset once the caller accepts the owner's exposure. */
  guarded: Bytes32 | undefined;
  exitTokenOwner: Address | undefined;
}>;

async function hiddenOwner(
  input: Readonly<{
    rail: AnonymousRail;
    deposits: readonly TransactDeposit[] | undefined;
    client: Pick<RingProvingClient, "getAccount">;
    sender: ShieldedAddress;
    prepared: PreparedTransfer;
  }>,
  context: RequestContext | undefined,
): Promise<HiddenOwner> {
  const { rail, prepared } = input;
  const owner = checkAnonymousSigner({ signer: rail.signer, sender: input.sender });
  // The proof no longer holds the payer, a funder or the exit apart from the hidden owner, each
  // would name it, so the wallet refuses them unless the caller accepts the exposure.
  const guarded = rail.exposure === "refused" ? owner : undefined;
  if (guarded !== undefined)
    checkHiddenParties(guarded, [
      prepared.payer,
      ...(input.deposits ?? []).map(({ funder }) => signerAddress(funder)),
    ]);
  const exitTokenOwner = await anonymousExitTokenOwner(
    {
      client: input.client,
      transfers: prepared.interfaceTransfers,
      ...(guarded === undefined ? {} : { hiddenOwner: guarded }),
      ...(rail.exitTokenOwner === undefined ? {} : { tokenOwner: rail.exitTokenOwner }),
    },
    context,
  );
  return { owner, guarded, exitTokenOwner };
}

type SealedOwners = Readonly<{
  inProofOwner: Bytes32 | undefined;
  hidden: HiddenOwner | undefined;
}>;

function checkSealedInputs(
  rail: OwnerRail,
  sealed: SealedOwners & Readonly<{ proofInputs: SppProofInputs; sender: ShieldedAddress }>,
): void {
  const { proofInputs, sender } = sealed;
  if (rail.kind === "confidential") {
    checkZeroKeyInputs(proofInputs.inputUtxos, sender.signingPublicKey);
    if (sealed.inProofOwner !== undefined) checkRingNotes(proofInputs);
  }
  if (rail.kind !== "anonymous") return;
  // A cached write needs a public writer signing beside the hidden owner, and no party cell or
  // signer class names one. A cached read and an address slot name no signer.
  if (proofInputs.outputs.some((output) => output.cacheSlot !== undefined))
    throw new RingError("RING_TRANSACT_DATA_INVALID", {
      details: { reason: "InvalidAnonymousSlot" },
    });
  // A zero-key note opens to anyone holding its opening, its spend names the spender.
  if (sealed.hidden?.guarded !== undefined)
    checkZeroKeyInputs(proofInputs.inputUtxos, sender.signingPublicKey);
  checkHiddenOwnerSpends(proofInputs, sender);
  checkEntryInputs(proofInputs, sender);
}

function stagedSpend(
  rail: OwnerRail,
  ring: SealedOwners & Readonly<{ anonymous: boolean }>,
): Omit<RailSpend, "prove"> {
  const { inProofOwner, hidden } = ring;
  if (rail.kind === "confidential")
    // SPP's anonymous rail hides the owner, the statement publishes it.
    return { rail: "confidential", ...(inProofOwner === undefined ? {} : { inProofOwner }) };
  if (rail.kind === "publicOwners")
    return ring.anonymous
      ? {
          rail: "anonymous",
          publicOwners: true,
          anonymous: { partyIdentities: (coSigners) => anonymousPartyIdentities(coSigners) },
        }
      : { rail: "confidential" };
  return {
    rail: "anonymous",
    anonymous: {
      ...(hidden?.exitTokenOwner === undefined ? {} : { tokenOwner: hidden.exitTokenOwner }),
      // A hidden co-signer approves its own outflow, no exposure waiver admits it.
      partyIdentities: (coSigners: readonly Address[]) => {
        if (
          hidden !== undefined &&
          coSigners.some((coSigner) => equalBytes(addressBytes(coSigner), hidden.owner))
        )
          throw new RingError("RING_CO_SIGNER_INVALID", {
            details: { reason: "CoSignerIsOwner", code: RingProgramError.coSignerIsOwner },
          });
        return anonymousPartyIdentities(coSigners);
      },
    },
  };
}

type OutputSeal = Readonly<{
  key: ViewingKey;
  prepared: PreparedTransfer;
  /** Read inside the seal, a refusal destroys `key`. */
  delivery: () => RingDelivery;
  payments: readonly AnonymousPayment[];
  /** Fixed by a program commitment or its plan. */
  salt: Bytes16 | undefined;
  sender: ShieldedAddress;
  assets: AssetRegistry;
  auditor: X25519PublicKey;
  instructionDiscriminator: number;
  zeroChangeAsset: Address | undefined;
}>;

/** A secret a staged transfer holds is destroyed on every exit, `key` here once sealing or `checked` throws. */
export function sealOutputs<Checked>(
  seal: OutputSeal,
  checked: (rail: RingOutputRail, proofInputs: SppProofInputs) => Checked,
): Checked {
  const { key, prepared } = seal;
  try {
    const rail = new RingOutputRail(
      key,
      prepared.firstNullifier,
      seal.delivery(),
      seal.payments,
      seal.salt,
    );
    return checked(
      rail,
      rail.encrypt({
        prepared,
        sender: seal.sender,
        assets: seal.assets,
        auditor: seal.auditor,
        instructionDiscriminator: seal.instructionDiscriminator,
        ...(seal.zeroChangeAsset === undefined ? {} : { zeroChangeAsset: seal.zeroChangeAsset }),
      }),
    );
  } catch (cause) {
    key.destroy();
    throw cause;
  }
}

/** A secret a staged transfer holds is destroyed on every exit, success or throw. */
export async function proveSealed(
  rail: RingOutputRail,
  staged: () => StagedTransferInput,
  context: RequestContext | undefined,
): Promise<ProvenRingTransfer> {
  try {
    return await proveStagedTransfer(staged(), context);
  } finally {
    rail.txViewingKey.destroy();
  }
}

type RailProver = (stage: StagedRail) => RailSpend["prove"];

export async function proveRail(
  input: Readonly<{
    transfer: RailTransfer;
    client: RingProvingClient;
    prover: RailProver;
    approval: RingTransferApproval | undefined;
  }>,
  context: RequestContext | undefined,
): Promise<ProvenRingTransfer> {
  const { client, prover, approval } = input;
  const { owner } = input.transfer;
  const stage = await stageTransfer(input.transfer, client, context);
  return proveSealed(
    stage.staged.rail,
    () => ({
      client,
      ringProgramId: owner.ringProgramId,
      config: stage.config,
      staged: {
        ...stage.staged,
        ...stagedBlinder(stage.approvalBlinder),
        spend: { ...stage.spend, prove: prover(stage) },
      },
      ...suppliedCoSigners(owner.coSigners),
      listSalts: owner.listSalts ?? [],
      ...suppliedVoucher(owner),
      ...ringSalted(owner.ringSalt),
      ...approvedStage(approval, stage.prepared),
    }),
    context,
  );
}

/** Each input owner signs the transaction. */
export function signedSpend(transfer: RailTransfer, client: PublicOwnerTransferClient): RailProver {
  const { ringProgramId, keys } = transfer.owner;
  return ({ staged }) =>
    (inputs, proveContext) =>
      client.proveRingTransact(
        inputs,
        ringProgramId,
        ringSpendAuthority({
          keys,
          coOwners: transfer.coOwners,
          proofInputs: inputs,
          sender: staged.sender,
        }),
        { outputTree: client },
        proveContext,
      );
}

/** `signer` signs the final inputs inside the SPP proof. */
export function inProofSpend(
  input: Readonly<{
    transfer: RailTransfer;
    client: AnonymousTransferClient;
    signer: AnonymousSpendSigner;
  }>,
): RailProver {
  const { transfer, client, signer } = input;
  const { ringProgramId, keys } = transfer.owner;
  return ({ staged, inProofOwner }) => {
    if (inProofOwner === undefined) throw new RingError("RING_IN_PROOF_OWNER_MISMATCH");
    return async (inputs, proveContext) =>
      client.proveAnonymousRingTransact(
        {
          proofInputs: inputs,
          ringProgramId,
          owner: await preparedOwnerSignature(signer, inProofOwner, inputs, proveContext),
          keys: ringSpendAuthority({
            keys,
            coOwners: transfer.coOwners,
            proofInputs: inputs,
            sender: staged.sender,
          }),
          config: { outputTree: client },
        },
        proveContext,
      );
  };
}

type RelayWire = Pick<
  RelayProofInputs,
  "rail" | "leafIndexes" | "interfaceTransferAccounts" | "coOwners" | "approvalBlinder"
>;

type RelayDelivery<Relayed> = Readonly<{
  wire: RelayWire;
  /** `payload` is wiped once it returns. */
  finish(payload: Uint8Array): Relayed | Promise<Relayed>;
}>;

type RelayFrame<Relayed> = Readonly<{
  quote: RelayQuote;
  nullifierKey: NullifierKey;
  sealRow?: SealRow;
  /** Refusals over the seal, ahead of its wire inputs. */
  deliver(sealed: SealedTransfer, stage: StagedRail): Promise<RelayDelivery<Relayed>>;
}> &
  (
    | Readonly<{ kind: "spend" }>
    /** The frame's sender proves nothing, each funder signs every output and leg of the frame. */
    | Readonly<{ kind: "deposit"; funders: readonly Address[] }>
  );

/**
 * A secret a staged transfer holds is destroyed on every exit, success or throw, the transaction
 * key, the seal's ephemeral secret and the encoded payload alike.
 */
async function relayRail<Relayed>(
  input: Readonly<{
    transfer: RailTransfer;
    client: Pick<RingProvingClient, "getAccount">;
    frame: RelayFrame<Relayed>;
  }>,
  context: RequestContext | undefined,
): Promise<Relayed> {
  const { transfer, frame } = input;
  const { owner } = transfer;
  const { ringProgramId } = owner;
  if (frame.kind === "spend") checkRelayUnvouched(owner);
  const stage = await stageTransfer(transfer, input.client, context);
  const { config, prepared, staged } = stage;
  try {
    if (frame.kind === "deposit") checkRelayUnvouched(owner);
    const { quote } = frame;
    if (prepared.payer !== quote.feePayer)
      throw new RingError("RING_RELAY_PAYER_MISMATCH", {
        details: { payer: prepared.payer, feePayer: quote.feePayer },
      });
    // The fee payer signs whatever the relay sends, a leg naming it would fund the note from the relay.
    if (frame.kind === "deposit" && frame.funders.includes(quote.feePayer))
      throw new RingError("RING_RELAY_FEE_PAYER_FUNDS_DEPOSIT");
    const { rail, inProofOwner, publicOwners } = stage.spend;
    const sealed = sealRelayedTransfer({
      ringProgramId,
      config,
      staged: {
        ...staged,
        spend:
          frame.kind === "deposit"
            ? { rail, ...(publicOwners === undefined ? {} : { publicOwners }) }
            : { rail, ...(inProofOwner === undefined ? {} : { inProofOwner }) },
      },
      ...(frame.sealRow === undefined ? {} : { sealRow: frame.sealRow }),
    });
    let payload: Uint8Array | undefined;
    try {
      const { proofInputs } = sealed;
      const delivery = await frame.deliver(sealed, stage);
      payload = encodeRelayProofInputs({
        ...delivery.wire,
        ringProgramId,
        sender: staged.sender,
        nullifierKey: frame.nullifierKey,
        txViewingKey: staged.rail.txViewingKey,
        inputs: proofInputs.inputUtxos,
        outputs: proofInputs.outputs,
        externalData: proofInputs.externalData,
        blindingSeed: proofInputs.blindingSeed,
        outputTreeId: proofInputs.outputTreeId,
        wideOwners: sealed.plan.wideOwners,
        assets: owner.assets,
        listSalts: relayListSalts(config, owner.listSalts ?? []),
      });
      return await delivery.finish(payload);
    } finally {
      payload?.fill(0);
      sealed.sealed.ephemeralSecret.fill(0);
    }
  } finally {
    staged.rail.txViewingKey.destroy();
  }
}

/** Rust `prepare_relay`, the owner signs inside the proof Ring RPC builds. */
export async function relaySpend(
  input: Readonly<{
    transfer: RailTransfer<ConfidentialRail | AnonymousRail>;
    client: Pick<RingProvingClient, "getAccount">;
    relay: RingRelayInput;
    approval: RingTransferApproval | undefined;
  }>,
  context: RequestContext | undefined,
): Promise<UnsignedRelay> {
  const { transfer, relay, approval } = input;
  const { owner, coOwners, rail } = transfer;
  const { ringProgramId } = owner;
  const signer = rail.kind === "anonymous" ? rail.signer : rail.inProofOwner;
  if (signer === undefined) throw new RingError("RING_IN_PROOF_OWNER_MISMATCH");
  const { quote } = relay;
  return relayRail(
    {
      transfer,
      client: input.client,
      frame: {
        kind: "spend",
        quote,
        nullifierKey: relay.nullifierKey,
        ...(relay.sealRow === undefined ? {} : { sealRow: relay.sealRow }),
        deliver: async (sealed, { prepared, staged, approvalBlinder }) => {
          const { proofInputs } = sealed;
          await approval?.approve({ prepared, proofInputs, coSigners: [] });
          const wire = {
            rail: spendWireRail(rail, staged.rail.delivery),
            leafIndexes: proofInputs.inputUtxos.map((spent) => leafIndexOf(spent, relay.notes)),
            interfaceTransferAccounts: staged.withdrawal === undefined ? [] : [staged.withdrawal],
            coOwners,
            ...stagedBlinder(approvalBlinder),
          };
          const finish = async (payload: Uint8Array): Promise<UnsignedRelay> => {
            const coSigners = owner.coSigners ?? [];
            const terms = sealedTransferTerms({
              ringProgramId,
              feePayer: quote.feePayer,
              sealed,
              staged,
              setup: await tokenAccountCreates(relay.tokenAccountSetup, staged.withdrawal),
            });
            return new UnsignedRelay({
              signer,
              messageHash: proofInputs.messageHash(),
              request: {
                ring: ringProgramId,
                quote,
                ...(coSigners.length === 0 ? {} : { coSigners }),
                intent: {
                  operation: rail.kind,
                  proofInputs: base64.decode(payload) as RelayIntent["proofInputs"],
                  ...(relay.tokenAccountSetup === undefined
                    ? {}
                    : { tokenAccountSetup: relay.tokenAccountSetup }),
                },
              },
              expected: sealedExpectation(terms, approvalBlinder, sealed),
            });
          };
          return { wire, finish };
        },
      },
    },
    context,
  );
}

function spendWireRail(
  rail: ConfidentialRail | AnonymousRail,
  delivery: RingDelivery,
): RelayProofInputs["rail"] {
  if (rail.kind === "confidential") return { kind: "confidential" };
  if (delivery.kind !== "anonymous") throw new RingError("RING_NOT_ANONYMOUS");
  return {
    kind: "anonymous",
    plan: delivery.plan,
    ...(rail.exitTokenOwner === undefined ? {} : { exitTokenOwner: rail.exitTokenOwner }),
    ownerExposed: rail.exposure === "accepted",
  };
}

/** One funder term per funding key, Ring RPC builds the frame and the funder signs it. */
export function relayDeposit(
  input: Readonly<{
    transfer: RailTransfer;
    client: Pick<RingProvingClient, "getAccount">;
    relay: DepositRelayInput;
  }>,
  context: RequestContext | undefined,
): Promise<DepositRelayFrame> {
  const { transfer, relay } = input;
  const { owner } = transfer;
  const { quote } = relay;
  const deposits = owner.deposits ?? [];
  const funders = [...new Set(deposits.map(({ funder }) => signerAddress(funder)))];
  return relayRail(
    {
      transfer,
      client: input.client,
      frame: {
        kind: "deposit",
        quote,
        nullifierKey: relay.nullifierKey,
        funders,
        deliver: async (sealed, { staged }) => {
          const { proofInputs } = sealed;
          const plan = staged.rail.delivery;
          const legs = proofInputs.externalData.interfaceTransfers;
          const wire: RelayWire = {
            rail: { kind: "deposit", ...(plan.kind === "anonymous" ? { plan: plan.plan } : {}) },
            leafIndexes: proofInputs.inputUtxos.map(() => 0n),
            interfaceTransferAccounts: deposits.map(relayLegAccounts),
            coOwners: [],
          };
          const finish = (payload: Uint8Array): DepositRelayFrame => {
            const expected: ExpectedSealed = Object.freeze({
              kind: "sealed",
              terms: [],
              funders: sealedFunderTerms({
                ringProgramId: owner.ringProgramId,
                feePayer: quote.feePayer,
                sealed,
                deposits,
              }),
              chargedOutflows: chargedOutflowsOf(proofInputs),
            });
            // The caller owns the copy, the encoded bytes are wiped with the frame.
            return Object.freeze({ payload: payload.slice(), legs: relayLegs(legs), expected });
          };
          return { wire, finish };
        },
      },
    },
    context,
  );
}

function suppliedCoSigners(
  coSigners: readonly Address[] | undefined,
): Pick<StagedTransferInput, "coSigners"> {
  return coSigners === undefined ? {} : { coSigners };
}

function suppliedVoucher(
  input: Pick<CustomRingTransferInput, "voucher">,
): Pick<StagedTransferInput, "voucher"> {
  return input.voucher === undefined ? {} : { voucher: input.voucher };
}

/** Ring RPC proves a relayed transfer unvouched. */
function checkRelayUnvouched(input: Pick<CustomRingTransferInput, "voucher">): void {
  if (input.voucher !== undefined) throw new RingError("RING_RELAY_VOUCHER_UNSUPPORTED");
}

/** Drawn per attempt, a retry never reuses a blinder a co-signer has seen. */
function attemptBlinder(input: CustomRingTransferInput): Bytes32 | undefined {
  return input.exactApproval === true ? randomBlinding() : undefined;
}

function stagedBlinder(blinder: Bytes32 | undefined): Readonly<{ approvalBlinder?: Bytes32 }> {
  return blinder === undefined ? {} : { approvalBlinder: blinder };
}

/** The relay builds the frame, the sealing wallet hands the opening to its co-signer. */
function sealedExpectation(
  terms: readonly RelayTransferTerms[],
  blinder: Bytes32 | undefined,
  sealed: Pick<SealedTransfer, "proofInputs" | "plan">,
): ExpectedSealed {
  return Object.freeze({
    kind: "sealed",
    terms,
    chargedOutflows: chargedOutflowsOf(sealed.proofInputs),
    ...(blinder === undefined
      ? {}
      : {
          approval: transferApprovalOf(sealed.proofInputs, sealed.plan.layout.capacity, blinder)
            .opening,
        }),
  });
}

export function programPlan(
  stage: Readonly<{ staged: Omit<StagedTransfer, "spend"> }>,
): ProgramPlan {
  stage.staged.rail.txViewingKey.destroy();
  return new ProgramPlan(stage.staged.proofInputs);
}

/** The staged hashes equal the committed ones before any proof, the co-signers then approve the program's call. */
function committedStage(
  program: ProgramCommitment | undefined,
  proofInputs: SppProofInputs,
): Readonly<{ coSignerForm?: CoSignerForm }> {
  if (program === undefined) return {};
  checkProgramCommitment(program, proofInputs);
  return { coSignerForm: "approval" };
}

/** After sealing, a slot's cache source changes no blinding and no output. */
function withReadCacheSlots(proofInputs: SppProofInputs, readCache: ReadCache): SppProofInputs {
  const inputUtxos = proofInputs.inputUtxos.slice();
  for (const note of readCache.notes) {
    const at = inputUtxos.findIndex((spent) => equalBytes(spent.nullifier(), note.nullifier));
    const spent = inputUtxos[at];
    if (spent === undefined)
      throw new RingError("RING_TRANSACT_DATA_INVALID", {
        details: { reason: "CachedInputUnknown" },
      });
    inputUtxos[at] = spent.withCacheSlot(note.slot);
  }
  return new SppProofInputs({
    payer: proofInputs.payer,
    inputUtxos,
    outputs: proofInputs.outputs,
    externalData: proofInputs.externalData,
    blindingSeed: proofInputs.blindingSeed,
    outputTreeId: proofInputs.outputTreeId,
  }).withReadCache(readCache.cache);
}

/** Rust `RelayProofInputs::token_account_setup`, each account the withdrawal pays. */
async function tokenAccountCreates(
  setup: readonly TokenAccountSetup[] | undefined,
  withdrawal: TransactWithdrawal | undefined,
): Promise<readonly TokenAccountCreate[]> {
  const created = new Set<Address>();
  return Promise.all(
    (setup ?? []).map(async (entry, index) => {
      const refused = (): RingError =>
        new RingError("RING_RELAY_PROOF_INPUTS", {
          details: { reason: "tokenAccountSetup", index },
        });
      if (withdrawal?.kind !== "spl" || withdrawal.mint !== entry.mint) throw refused();
      const { tokenProgram } = withdrawal;
      const account = await associatedTokenAddress(entry.owner, entry.mint, tokenProgram);
      if (account !== withdrawal.recipientTokenAccount || created.has(account)) throw refused();
      created.add(account);
      return Object.freeze({ owner: entry.owner, mint: entry.mint, tokenProgram });
    }),
  );
}

/** The approval a pipeline runs, bound to the prepared transfer the rail proves. */
export function approvedStage(
  approval: RingTransferApproval | undefined,
  prepared: PreparedTransfer,
): Pick<StagedTransferInput, "transaction" | "approve"> {
  if (approval === undefined) return {};
  return {
    transaction: approval.transaction,
    approve: (coSigners, proofInputs) => approval.approve({ prepared, proofInputs, coSigners }),
  };
}

/** The ring's config once SPP's registration admits it, Rust `require_config` and `require_active`. */
export async function activeConfig(
  client: Pick<RingProvingClient, "getAccount">,
  ringProgramId: Address,
  context: RequestContext | undefined,
): Promise<RingPolicy> {
  const [config] = await Promise.all([
    fetchRingPolicy(client, ringProgramId, context),
    requireActiveRing(client, ringProgramId, context),
  ]);
  return config;
}

/** The sender's transaction key, derived by its key holder from the first nullifier. */
async function transactionKey(
  keys: WalletKeys,
  sender: ShieldedAddress,
  prepared: PreparedTransfer,
  context: RequestContext | undefined,
): Promise<ViewingKey> {
  const derived = await keys.transactionKeys(
    [{ viewingPublicKey: sender.viewingPublicKey, firstNullifier: prepared.firstNullifier }],
    context,
  );
  const [key, ...extra] = derived;
  for (const surplus of extra) surplus.destroy();
  if (key === undefined)
    throw new RingError("RING_KEYS_REQUIRED", { details: { reason: "transactionKey" } });
  return key;
}

/** A public withdrawal names its settlement accounts, a transfer without one names none. */
export function checkWithdrawalAccounts(
  prepared: PreparedTransfer,
  withdrawal: TransactWithdrawal | undefined,
): void {
  const leg = prepared.interfaceTransfers.find((transfer) => !transfer.isDeposit);
  const matches =
    leg === undefined
      ? withdrawal === undefined
      : withdrawal !== undefined &&
        (leg.kind === "sol"
          ? withdrawal.kind === "sol" && withdrawal.recipient === leg.userSolAccount
          : withdrawal.kind === "spl" &&
            withdrawal.mint === leg.mint &&
            withdrawal.recipientTokenAccount === leg.tokenAccount);
  if (!matches) throw new RingError("RING_SETTLEMENT_ACCOUNTS_INVALID");
}

/** Each deposit leg names its funder's accounts in leg order, Rust `funder_account_position`. */
function checkDepositAccounts(
  prepared: PreparedTransfer,
  deposits: readonly TransactDeposit[] | undefined,
): void {
  const legs = prepared.interfaceTransfers.filter((transfer) => transfer.isDeposit);
  const accounts = deposits ?? [];
  const matches =
    legs.length === accounts.length &&
    legs.every((leg, index) => {
      const named = accounts[index];
      return leg.kind === "sol"
        ? named?.kind === "sol" && signerAddress(named.funder) === leg.userSolAccount
        : named?.kind === "spl" &&
            named.mint === leg.mint &&
            named.sourceTokenAccount === leg.tokenAccount;
    });
  if (!matches) throw new RingError("RING_SETTLEMENT_ACCOUNTS_INVALID");
}

function stagedDeposits(
  input: CustomRingTransferInput,
): Readonly<{ deposits?: readonly TransactDeposit[] }> {
  return input.deposits === undefined ? {} : { deposits: input.deposits };
}

/** The relay wire's account group of one deposit leg, a SOL leg names its funder. */
function relayLegAccounts(deposit: TransactDeposit): RelayLegAccounts {
  return deposit.kind === "sol"
    ? { kind: "sol", recipient: signerAddress(deposit.funder) }
    : {
        kind: "splDeposit",
        mint: deposit.mint,
        splTokenInterface: deposit.splTokenInterface,
        tokenAuthority: signerAddress(deposit.funder),
        userTokenAccount: deposit.sourceTokenAccount,
        tokenProgram: deposit.tokenProgram,
      };
}

/** A padding slot names no leaf. */
function leafIndexOf(input: ProofInputUtxo, notes: readonly WalletUtxo[]): bigint {
  if (input.isDummy()) return 0n;
  const note = notes.find((entry) => equalBytes(entry.nullifier, input.nullifier()));
  if (note === undefined)
    throw new RingError("RING_RELAY_PROOF_INPUTS", { details: { reason: "leafIndex" } });
  return note.outputContext.leafIndex;
}

function ringSalted(ringSalt: Bytes32 | undefined): Readonly<{ ringSalt?: Bytes32 }> {
  return ringSalt === undefined ? {} : { ringSalt };
}
