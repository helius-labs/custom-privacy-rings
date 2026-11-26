/** The rails of a ring transfer, Rust `RingTransfer<Owner>`, `<Anonymous>`, `<PublicOwners>` and `<Delegated>`. */

import {
  equalBytes,
  isZero,
  prepareRingAuthorityTransfer,
  addressBytes,
} from "@heliuslabs/spr/internal";
import type { AnonymousRingTransactProver } from "../../rpc/ports.js";
import type { ProofAuthority, Prover, WalletKeys } from "@heliuslabs/spr/client";
import { type Shape, InstructionTag } from "@heliuslabs/spr/interface";
import type {
  Address,
  Bytes32,
  RequestContext,
  TransactWithdrawal,
} from "@heliuslabs/spr/interface";
import type { NullifierKey, ShieldedAddress } from "@heliuslabs/spr/keypair";
import type { ExpectedSealed } from "../../rpc/relay-message.js";
import { randomSalt, ViewingKey } from "@heliuslabs/spr/keypair";
import type {
  AssetRegistry,
  ProofInputUtxo,
  ProofOutputUtxo,
  SettlementTransfer,
  WalletUtxo,
} from "@heliuslabs/spr/transaction";
import {
  createProofOutput,
  SOL_MINT,
  SppProofInputs,
  type ConfidentialTransfer,
  type PreparedTransfer,
  type WithdrawalTarget,
} from "@heliuslabs/spr/transaction";
import { type RelayLeg } from "../../protocol/ring-rpc/relay-proof-inputs.js";
import type { RelayQuote, TokenAccountSetup } from "../../protocol/ring-rpc/ring-rpc-types.js";
import { UnsignedRelay } from "../../rpc/relay.js";

import {
  type AnonymousPayment,
  type OwnerExposure,
  type AnonymousSpendSigner,
} from "./anonymous.js";
import type { VouchSigner } from "../../proof/vouch.js";
import { RingInstructionTag } from "../../protocol/transaction/instruction-tag.js";
import {
  isAnonymousFrame,
  ringRail,
  type FrameExtensions,
} from "../../protocol/transaction/transact.js";
import { RingError } from "../../protocol/error.js";
import { saltsByOwner } from "../../proof/policy-proof.js";
import { type RingListSalt } from "../../protocol/membership/policy.js";
import { anonymousPartyIdentities } from "../../protocol/membership/parties.js";
import {
  type ProvenRingTransfer,
  type RingProvingClient,
  type RingTransactionFrame,
  type TransactDeposit,
} from "../workflow/pipeline.js";
import { ProgramPlan, type ProgramCommitment } from "../workflow/program-commitment.js";
import type { SealRow } from "../workflow/transfer-plan.js";
import { declarePayment } from "./output-rail.js";
import { AnonymousPlan } from "../../protocol/transaction/anonymous-plan.js";
import { current, openLine, step } from "./transfer-line.js";
import {
  activeConfig,
  approvedStage,
  checkWithdrawalAccounts,
  inProofSpend,
  programPlan,
  proveRail,
  proveSealed,
  relayDeposit,
  relaySpend,
  sealOutputs,
  signedSpend,
  stageTransfer,
  type AnonymousRail,
  type ConfidentialRail,
  type Owned,
  type PublicOwnersRail,
  type RailTransfer,
} from "./rail-staging.js";
import { checkCoOwnerKeys, ringSpendAuthority, standIn } from "./ring-spend.js";
import { type CoOwner } from "../../protocol/ring-rpc/relay-proof-inputs.js";

/** Notes SPP reads from a cache account instead of proving their inclusion, Rust `ReadCache`. */
export interface ReadCache {
  readonly cache: Address;
  readonly notes: readonly Readonly<{ nullifier: Bytes32; slot: number }>[];
}

/** @internal */
export interface RingTransferApproval {
  readonly transaction: RingTransactionFrame;
  /** Runs once the transfer fits its transaction, before any proof. */
  approve(
    transfer: Readonly<{
      prepared: PreparedTransfer;
      proofInputs: SppProofInputs;
      coSigners: readonly Address[];
    }>,
  ): Promise<void>;
}

export type CustomRingTransferClient = RingProvingClient &
  Pick<Prover, "proveRingTransact"> &
  AnonymousRingTransactProver;

export type AnonymousTransferClient = RingProvingClient & AnonymousRingTransactProver;

export type DelegateTransferClient = RingProvingClient & Pick<Prover, "proveRingAuthorityTransact">;

/** Public owners settle on SPP's confidential rail on either ring mode. */
export type PublicOwnerTransferClient = RingProvingClient & Pick<Prover, "proveRingTransact">;

/** Rust `CustomRingTransferInput`. */
export interface CustomRingTransferInput {
  readonly ringProgramId: Address;
  /** Reads the sender's address and transaction keys and completes its inputs, no secret leaves it. */
  readonly keys: WalletKeys;
  /** Prepared by the rail, every output it already holds keeps its slot. */
  readonly transfer: ConfidentialTransfer;
  readonly assets: AssetRegistry;
  /** Read in order while a demanded class stays uncovered. */
  readonly coSigners?: readonly Address[];
  /** Every enrolled owner a salted list reads, the sender and the recipients alike. */
  readonly listSalts?: readonly RingListSalt[];
  /** A direct proof reads the private lists under it on a ring that commits its tag. */
  readonly ringSalt?: Bytes32;
  /** SPP's optional fields the frame carries through, the proof's own digests are set per attempt. */
  readonly extensions?: Omit<FrameExtensions, "approvalTotals" | "cache">;
  /** The frame carries a digest of its totals, Rust `with_exact_approval`. */
  readonly exactApproval?: boolean;
  /** The owner of the withdrawal's token account, the policy read names it while the transaction creates that account. */
  readonly withdrawalOwner?: Address;
  /** The settlement accounts of each deposit leg `transfer` names, in leg order. */
  readonly deposits?: readonly TransactDeposit[];
  /** A listed wallet vouching for the transfer. */
  readonly voucher?: VouchSigner;
  /** The commitment of the program calling the transfer, Rust `with_program`. */
  readonly program?: ProgramCommitment;
}

/** A payment's address carries its recipient's list salt, none under the zero key, a salt the builder holds otherwise is refused. */
function withPaymentSalt(owner: Owned, payment: AnonymousPayment): Owned {
  const { recipient, listSalt } = payment.to;
  if (isZero(listSalt)) return owner;
  const salt = { owner: recipient.signingPublicKey.ownerProofInputHash(), salt: listSalt };
  const listSalts = [...(owner.listSalts ?? []), salt];
  saltsByOwner(listSalts);
  return { ...owner, listSalts };
}

/** Every ring output owner other than the sender is paid through one step, Rust `with_payment`. */
function payOn(line: Owned, payment: AnonymousPayment) {
  return step(
    line,
    (owner) => {
      checkPaymentRing(payment, owner.ringProgramId);
      return withPaymentSalt(owner, payment);
    },
    (salted) => declarePayment(salted.transfer, payment),
  );
}

/** The confidential rail, the owner signs its spends in public or inside the proof, Rust `RingTransfer<Owner>`. */
export class CustomRingTransfer {
  readonly #held: Owned;
  readonly #withdrawal: TransactWithdrawal | undefined;
  readonly #inProofOwner: AnonymousSpendSigner | undefined;
  readonly #coOwners: readonly CoOwner[];

  private constructor(
    input: Owned,
    withdrawal: TransactWithdrawal | undefined,
    inProofOwner: AnonymousSpendSigner | undefined,
    coOwners: readonly CoOwner[] = [],
  ) {
    this.#held = input;
    this.#withdrawal = withdrawal;
    this.#inProofOwner = inProofOwner;
    this.#coOwners = coOwners;
    Object.freeze(this);
  }

  get #input(): Owned {
    return current(this.#held);
  }

  /** A second line supersedes the first until a ring step wrote into `transfer`, then it is refused. */
  static new(input: CustomRingTransferInput): CustomRingTransfer {
    const line = openLine(input, (transfer) => {
      transfer.withRingProgramId(input.ringProgramId);
    });
    return new CustomRingTransfer(line, undefined, undefined);
  }

  /** @internal The line writes the output tree and then holds `transfer`, a later line over it is refused. */
  withOutputTree(outputTreeId: number): CustomRingTransfer {
    return this.#with(
      step(
        this.#held,
        (owner) => owner,
        ({ transfer }) => {
          transfer.withOutputTreeId(outputTreeId);
        },
      ).line,
    );
  }

  /** @internal A delivered payment of the signer rail, written by the line that holds `transfer`. */
  withOutput(
    output: Readonly<{ recipient: ShieldedAddress; asset: Address; amount: bigint }>,
  ): CustomRingTransfer {
    return this.#with(
      step(
        this.#held,
        (owner) => owner,
        ({ transfer }) => {
          transfer.send(output.recipient, output.asset, output.amount);
        },
      ).line,
    );
  }

  /** The frame carries a digest of its totals, the opening reads as `approvalOpening`. */
  withExactApproval(): CustomRingTransfer {
    return this.#with({ ...this.#input, exactApproval: true });
  }

  /** `voucher` signs the transfer inside the proof, its allow entry stands in for every sender and payment owner. */
  withVoucher(voucher: VouchSigner): CustomRingTransfer {
    return this.#with({ ...this.#input, voucher });
  }

  /** Staging seals under the committed salt and refuses another hash, each co-signer approves the program's call. */
  withProgram(commitment: ProgramCommitment): CustomRingTransfer {
    return this.#with({ ...this.#input, program: commitment });
  }

  /** Draws the salt, a transfer changed after the plan stages another hash, Rust `plan_for_program`. */
  async planForProgram(
    client: Pick<RingProvingClient, "getAccount">,
    context?: RequestContext,
  ): Promise<ProgramPlan> {
    return programPlan(await stageTransfer(this.#rail(), client, context, randomSalt()));
  }

  /** The settlement accounts of the transfer's public withdrawal, Rust `with_interface_transfer_accounts`. */
  withWithdrawalAccounts(withdrawal: TransactWithdrawal, owner?: Address): CustomRingTransfer {
    const input = owner === undefined ? this.#input : { ...this.#input, withdrawalOwner: owner };
    return new CustomRingTransfer(input, withdrawal, this.#inProofOwner, this.#coOwners);
  }

  /** The sender's signing key signs inside the proof, any fee payer sends the transaction, Rust `with_in_proof_owner`. */
  withInProofOwner(signer: AnonymousSpendSigner): CustomRingTransfer {
    return new CustomRingTransfer(this.#input, this.#withdrawal, signer, this.#coOwners);
  }

  /** A co-owner of `ringSpendTransfer` inputs signs the transaction. */
  withCoOwner(coOwner: CoOwner): CustomRingTransfer {
    return new CustomRingTransfer(this.#input, this.#withdrawal, this.#inProofOwner, [
      ...this.#coOwners,
      Object.freeze({ ...coOwner }),
    ]);
  }

  /** `wallet` signs in public and may spend its zero-key notes, nothing selects them for it, Rust `spend_public`. */
  spendPublic(wallet: Address): PublicOwnerTransfer {
    return PublicOwnerTransfer.of({
      owner: this.#input,
      ...(this.#withdrawal === undefined ? {} : { withdrawal: this.#withdrawal }),
      wallet,
      inProofOwner: this.#inProofOwner !== undefined,
      coOwners: this.#coOwners,
    });
  }

  /** The hidden Ed25519 owner an anonymous ring proves, it must be the sender's signing key. */
  withAnonymousSigner(signer: AnonymousSpendSigner): AnonymousTransfer {
    return AnonymousTransfer.of({
      owner: this.#input,
      ...(this.#withdrawal === undefined ? {} : { withdrawal: this.#withdrawal }),
      signer,
      coOwners: this.#coOwners,
    });
  }

  prove(client: CustomRingTransferClient, context?: RequestContext): Promise<ProvenRingTransfer> {
    return this.#prove(client, undefined, context);
  }

  /** @internal */
  proveApproved(
    client: CustomRingTransferClient,
    approval: RingTransferApproval,
    context?: RequestContext,
  ): Promise<ProvenRingTransfer> {
    return this.#prove(client, approval, context);
  }

  /** Rust `prepare_relay`, the wallet reads no Merkle path and no record. */
  prepareRelay(
    client: Pick<RingProvingClient, "getAccount">,
    input: RingRelayInput,
    context?: RequestContext,
  ): Promise<UnsignedRelay> {
    return this.#prepareRelay(client, input, undefined, context);
  }

  /** @internal */
  prepareRelayApproved(
    client: Pick<RingProvingClient, "getAccount">,
    input: RingRelayInput,
    approval: RingTransferApproval,
    context?: RequestContext,
  ): Promise<UnsignedRelay> {
    return this.#prepareRelay(client, input, approval, context);
  }

  /** @internal Rust `deposit_relay`, the wallet reads no record. */
  async depositRelay(
    client: Pick<RingProvingClient, "getAccount">,
    input: DepositRelayInput,
    context?: RequestContext,
  ): Promise<DepositRelayFrame> {
    return relayDeposit({ transfer: this.#rail(), client, relay: input }, context);
  }

  async #prepareRelay(
    client: Pick<RingProvingClient, "getAccount">,
    input: RingRelayInput,
    approval: RingTransferApproval | undefined,
    context: RequestContext | undefined,
  ): Promise<UnsignedRelay> {
    return relaySpend({ transfer: this.#rail(), client, relay: input, approval }, context);
  }

  async #prove(
    client: CustomRingTransferClient,
    approval: RingTransferApproval | undefined,
    context: RequestContext | undefined,
  ): Promise<ProvenRingTransfer> {
    const transfer = this.#rail();
    const signer = this.#inProofOwner;
    const prover =
      signer === undefined
        ? signedSpend(transfer, client)
        : inProofSpend({ transfer, client, signer });
    return proveRail({ transfer, client, prover, approval }, context);
  }

  #with(input: Owned): CustomRingTransfer {
    return new CustomRingTransfer(input, this.#withdrawal, this.#inProofOwner, this.#coOwners);
  }

  #rail(): RailTransfer<ConfidentialRail> {
    return {
      owner: this.#input,
      withdrawal: this.#withdrawal,
      coOwners: this.#coOwners,
      rail: { kind: "confidential", inProofOwner: this.#inProofOwner },
    };
  }
}

/** The hidden owner's spends on an anonymous ring, no transaction signer names the owner, Rust `RingTransfer<Anonymous>`. */
export class AnonymousTransfer {
  readonly #held: Owned;
  readonly #withdrawal: TransactWithdrawal | undefined;
  readonly #signer: AnonymousSpendSigner;
  /** In declared order, the order of their ring outputs. */
  readonly #payments: readonly AnonymousPayment[];
  readonly #shape: Shape | undefined;
  readonly #exitTokenOwner: Address | undefined;
  readonly #coOwners: readonly CoOwner[];
  readonly #readCache: ReadCache | undefined;
  readonly #exposure: OwnerExposure;

  private constructor(fields: AnonymousFields) {
    this.#held = fields.owner;
    this.#withdrawal = fields.withdrawal;
    this.#signer = fields.signer;
    this.#payments = fields.payments;
    this.#shape = fields.shape;
    this.#exitTokenOwner = fields.exitTokenOwner;
    this.#coOwners = fields.coOwners;
    this.#readCache = fields.readCache;
    this.#exposure = fields.exposure;
    Object.freeze(this);
  }

  get #owner(): Owned {
    return current(this.#held);
  }

  /** @internal */
  static of(
    input: Readonly<{
      owner: Owned;
      withdrawal?: TransactWithdrawal;
      signer: AnonymousSpendSigner;
      coOwners?: readonly CoOwner[];
    }>,
  ): AnonymousTransfer {
    return new AnonymousTransfer({
      owner: input.owner,
      withdrawal: input.withdrawal,
      signer: input.signer,
      payments: Object.freeze([]),
      shape: undefined,
      exitTokenOwner: undefined,
      coOwners: input.coOwners ?? [],
      readCache: undefined,
      exposure: "refused",
    });
  }

  /** A public co-owner of later `ringSpendTransfer` inputs signs the transaction. */
  withCoOwner(coOwner: CoOwner): AnonymousTransfer {
    return this.#with({ coOwners: [...this.#coOwners, Object.freeze({ ...coOwner })] });
  }

  /** Every ring output owner other than the sender is paid here, Rust `with_payment`. */
  withPayment(payment: AnonymousPayment): AnonymousTransfer {
    const fields = this.#fields();
    const paid = payOn(fields.owner, payment);
    return new AnonymousTransfer({
      ...fields,
      owner: paid.line,
      payments: Object.freeze([...this.#payments, paid.written]),
    });
  }

  /** Named notes spend from `cache`, SPP checks each slot against the frame's selector, Rust `with_read_cache`. */
  withReadCache(readCache: ReadCache): AnonymousTransfer {
    return this.#with({ readCache });
  }

  /** The frame carries a digest of its totals, the opening reads as `approvalOpening`. */
  withExactApproval(): AnonymousTransfer {
    return this.#with({ owner: { ...this.#owner, exactApproval: true } });
  }

  /** `voucher` signs the transfer inside the proof, its allow entry stands in for every sender and payment owner. */
  withVoucher(voucher: VouchSigner): AnonymousTransfer {
    return this.#with({ owner: { ...this.#owner, voucher } });
  }

  /** Pads to `shape`, else the transfer takes the smallest supported shape. */
  withShape(shape: Shape): AnonymousTransfer {
    const fields = this.#fields();
    const shaped = step(
      fields.owner,
      (owner) => owner,
      ({ transfer }) => {
        transfer.withShape(shape);
      },
    );
    return new AnonymousTransfer({ ...fields, owner: shaped.line, shape });
  }

  /** A missing exit token account must be the owner's associated token account. */
  withExitTokenOwner(owner: Address): AnonymousTransfer {
    return this.#with({ exitTokenOwner: owner });
  }

  /** Admits the parties and zero-key spends `OwnerExposure` names, a co-signer stays refused, Rust `with_owner_exposure`. */
  withOwnerExposure(exposure: OwnerExposure): AnonymousTransfer {
    return this.#with({ exposure });
  }

  prove(client: AnonymousTransferClient, context?: RequestContext): Promise<ProvenRingTransfer> {
    return this.#prove(client, undefined, context);
  }

  /** @internal */
  proveApproved(
    client: AnonymousTransferClient,
    approval: RingTransferApproval,
    context?: RequestContext,
  ): Promise<ProvenRingTransfer> {
    return this.#prove(client, approval, context);
  }

  /** Rust `prepare_relay`, the wallet reads no Merkle path and no record. */
  async prepareRelay(
    client: Pick<RingProvingClient, "getAccount">,
    input: RingRelayInput,
    context?: RequestContext,
  ): Promise<PreparedRingRelay> {
    return this.#prepareRelay(client, input, undefined, context);
  }

  /** @internal */
  prepareRelayApproved(
    client: Pick<RingProvingClient, "getAccount">,
    input: RingRelayInput,
    approval: RingTransferApproval,
    context?: RequestContext,
  ): Promise<PreparedRingRelay> {
    return this.#prepareRelay(client, input, approval, context);
  }

  async #prove(
    client: AnonymousTransferClient,
    approval: RingTransferApproval | undefined,
    context: RequestContext | undefined,
  ): Promise<ProvenRingTransfer> {
    const transfer = this.#rail();
    const prover = inProofSpend({ transfer, client, signer: this.#signer });
    return proveRail({ transfer, client, prover, approval }, context);
  }

  async #prepareRelay(
    client: Pick<RingProvingClient, "getAccount">,
    input: RingRelayInput,
    approval: RingTransferApproval | undefined,
    context: RequestContext | undefined,
  ): Promise<PreparedRingRelay> {
    return Object.freeze({
      relay: await relaySpend({ transfer: this.#rail(), client, relay: input, approval }, context),
    });
  }

  #with(changed: Partial<AnonymousFields>): AnonymousTransfer {
    return new AnonymousTransfer({ ...this.#fields(), ...changed });
  }

  #rail(): RailTransfer<AnonymousRail> {
    return {
      owner: this.#owner,
      withdrawal: this.#withdrawal,
      coOwners: this.#coOwners,
      rail: {
        kind: "anonymous",
        signer: this.#signer,
        payments: this.#payments,
        exitTokenOwner: this.#exitTokenOwner,
        readCache: this.#readCache,
        exposure: this.#exposure,
      },
    };
  }

  #fields(): AnonymousFields {
    return {
      owner: this.#owner,
      withdrawal: this.#withdrawal,
      signer: this.#signer,
      payments: this.#payments,
      shape: this.#shape,
      exitTokenOwner: this.#exitTokenOwner,
      coOwners: this.#coOwners,
      readCache: this.#readCache,
      exposure: this.#exposure,
    };
  }
}

interface AnonymousFields {
  readonly owner: Owned;
  readonly withdrawal: TransactWithdrawal | undefined;
  readonly signer: AnonymousSpendSigner;
  readonly payments: readonly AnonymousPayment[];
  readonly shape: Shape | undefined;
  readonly exitTokenOwner: Address | undefined;
  readonly coOwners: readonly CoOwner[];
  readonly readCache: ReadCache | undefined;
  readonly exposure: OwnerExposure;
}

/** Every live owner signs in public, recipients stay hidden on an anonymous ring, Rust `RingTransfer<PublicOwners>`. */
export class PublicOwnerTransfer {
  readonly #held: Owned;
  readonly #withdrawal: TransactWithdrawal | undefined;
  readonly #wallet: Address;
  readonly #inProofOwner: boolean;
  readonly #coOwners: readonly CoOwner[];
  /** In declared order, the order of their ring outputs. */
  readonly #payments: readonly AnonymousPayment[];

  private constructor(fields: PublicOwnerFields) {
    this.#held = fields.owner;
    this.#withdrawal = fields.withdrawal;
    this.#wallet = fields.wallet;
    this.#inProofOwner = fields.inProofOwner;
    this.#coOwners = fields.coOwners;
    this.#payments = fields.payments;
    Object.freeze(this);
  }

  get #owner(): Owned {
    return current(this.#held);
  }

  /** @internal */
  static of(
    input: Readonly<{
      owner: Owned;
      withdrawal?: TransactWithdrawal;
      wallet: Address;
      inProofOwner: boolean;
      coOwners: readonly CoOwner[];
    }>,
  ): PublicOwnerTransfer {
    return new PublicOwnerTransfer({
      ...input,
      withdrawal: input.withdrawal,
      payments: Object.freeze([]),
    });
  }

  /** Every ring output owner other than the wallet is paid here, Rust `with_payment`. */
  withPayment(payment: AnonymousPayment): PublicOwnerTransfer {
    const paid = payOn(this.#held, payment);
    return this.#with(paid.line, Object.freeze([...this.#payments, paid.written]));
  }

  /** The frame carries a digest of its totals, the opening reads as `approvalOpening`. */
  withExactApproval(): PublicOwnerTransfer {
    return this.#with({ ...this.#owner, exactApproval: true });
  }

  /** `voucher` signs the transfer inside the proof, its allow entry stands in for every sender and payment owner. */
  withVoucher(voucher: VouchSigner): PublicOwnerTransfer {
    return this.#with({ ...this.#owner, voucher });
  }

  /** Staging seals under the committed salt and refuses another hash, each co-signer approves the program's call. */
  withProgram(commitment: ProgramCommitment): PublicOwnerTransfer {
    return this.#with({ ...this.#owner, program: commitment });
  }

  /** Draws the salt, a transfer changed after the plan stages another hash, Rust `plan_for_program`. */
  async planForProgram(
    client: Pick<RingProvingClient, "getAccount">,
    context?: RequestContext,
  ): Promise<ProgramPlan> {
    return programPlan(await stageTransfer(this.#rail(), client, context, randomSalt()));
  }

  prove(client: PublicOwnerTransferClient, context?: RequestContext): Promise<ProvenRingTransfer> {
    return this.#prove(client, undefined, context);
  }

  /** @internal */
  proveApproved(
    client: PublicOwnerTransferClient,
    approval: RingTransferApproval,
    context?: RequestContext,
  ): Promise<ProvenRingTransfer> {
    return this.#prove(client, approval, context);
  }

  /** @internal Rust `deposit_relay`, the wallet reads no record. */
  async depositRelay(
    client: Pick<RingProvingClient, "getAccount">,
    input: DepositRelayInput,
    context?: RequestContext,
  ): Promise<DepositRelayFrame> {
    return relayDeposit({ transfer: this.#rail(), client, relay: input }, context);
  }

  async #prove(
    client: PublicOwnerTransferClient,
    approval: RingTransferApproval | undefined,
    context: RequestContext | undefined,
  ): Promise<ProvenRingTransfer> {
    const transfer = this.#rail();
    return proveRail(
      { transfer, client, prover: signedSpend(transfer, client), approval },
      context,
    );
  }

  #with(owner: Owned, payments: readonly AnonymousPayment[] = this.#payments): PublicOwnerTransfer {
    return new PublicOwnerTransfer({
      owner,
      withdrawal: this.#withdrawal,
      wallet: this.#wallet,
      inProofOwner: this.#inProofOwner,
      coOwners: this.#coOwners,
      payments,
    });
  }

  #rail(): RailTransfer<PublicOwnersRail> {
    return {
      owner: this.#owner,
      withdrawal: this.#withdrawal,
      coOwners: this.#coOwners,
      rail: {
        kind: "publicOwners",
        wallet: this.#wallet,
        inProofOwner: this.#inProofOwner,
        payments: this.#payments,
      },
    };
  }
}

interface PublicOwnerFields {
  readonly owner: Owned;
  readonly withdrawal: TransactWithdrawal | undefined;
  readonly wallet: Address;
  readonly inProofOwner: boolean;
  readonly coOwners: readonly CoOwner[];
  readonly payments: readonly AnonymousPayment[];
}

function checkPaymentRing(payment: AnonymousPayment, ringProgramId: Address): void {
  if (payment.to.ringProgramId !== ringProgramId)
    throw new RingError("RING_FOREIGN_RING", {
      details: { address: payment.to.ringProgramId, ring: ringProgramId },
    });
}

/** One output of a delegate move. */
export interface DelegateOutput {
  readonly recipient: ShieldedAddress;
  readonly asset: Address;
  readonly amount: bigint;
  /** Pays a default-ring note under SPP's `ring_authority_exit` circuit. */
  readonly exit?: boolean;
}

/** A delegate's public withdrawal, its settlement accounts named. */
export interface DelegateWithdrawal {
  readonly asset: Address;
  readonly amount: bigint;
  readonly target: WithdrawalTarget;
  readonly accounts: TransactWithdrawal;
}

/** Rust `DelegateTransferInput`. */
export interface DelegateTransferInput {
  readonly ringProgramId: Address;
  /** Signs the transaction beside the payer. */
  readonly delegate: Address;
  readonly payer: Address;
  /** The member whose notes move, its change returns to it. */
  readonly source: ShieldedAddress;
  /** Completes the source's inputs with the key the delegate recovered. */
  readonly sourceProofs: ProofAuthority;
  readonly inputs: readonly ProofInputUtxo[];
  /** Owners of the inputs `source` does not own. */
  readonly coSources?: readonly CoOwner[];
  /** The key the signed request names, fresh unless named. */
  readonly txViewingKey?: Bytes32;
  readonly outputs: readonly DelegateOutput[];
  readonly withdrawal?: DelegateWithdrawal;
  readonly assets: AssetRegistry;
  /** The outputs land in the first input's tree unless named. */
  readonly outputTreeId?: number;
}

/** The delegate moves members' notes with their keys, no member signs, Rust `RingTransfer<Delegated>`. */
export class DelegateTransfer {
  readonly #input: DelegateTransferInput;

  private constructor(input: DelegateTransferInput) {
    this.#input = input;
    Object.freeze(this);
  }

  static new(input: DelegateTransferInput): DelegateTransfer {
    return new DelegateTransfer(input);
  }

  withTxViewingKey(txViewingKey: Bytes32): DelegateTransfer {
    return new DelegateTransfer({ ...this.#input, txViewingKey });
  }

  withCoSource(coSource: CoOwner): DelegateTransfer {
    return new DelegateTransfer({
      ...this.#input,
      coSources: [...(this.#input.coSources ?? []), Object.freeze({ ...coSource })],
    });
  }

  prove(client: DelegateTransferClient, context?: RequestContext): Promise<ProvenRingTransfer> {
    return this.#prove(client, undefined, context);
  }

  /** @internal */
  proveApproved(
    client: DelegateTransferClient,
    approval: RingTransferApproval,
    context?: RequestContext,
  ): Promise<ProvenRingTransfer> {
    return this.#prove(client, approval, context);
  }

  async #prove(
    client: DelegateTransferClient,
    approval: RingTransferApproval | undefined,
    context: RequestContext | undefined,
  ): Promise<ProvenRingTransfer> {
    const input = this.#input;
    // The pinned core holds no exit key.
    if (input.outputs.some((output) => output.exit === true))
      throw new RingError("RING_EXIT_PROOF_UNAVAILABLE");
    const { ringProgramId } = input;
    const config = await activeConfig(client, ringProgramId, context);
    if (config.delegate === undefined)
      throw new RingError("RING_DELEGATE_INVALID", { details: { reason: "missing" } });
    if (config.delegate !== input.delegate)
      throw new RingError("RING_DELEGATE_INVALID", { details: { reason: "unauthorized" } });
    const railKind = ringRail(RingInstructionTag.authorityTransfer, config);
    const coSources = input.coSources ?? [];
    if (
      input.inputs.some(
        (spent) =>
          !spent.utxo.owner.equals(input.source.signingPublicKey) &&
          !coSources.some((coSource) => ownedBy(spent, coSource.owner)),
      )
    )
      throw new RingError("RING_DELEGATE_INVALID", { details: { reason: "mixedOwners" } });
    const first = input.inputs[0];
    if (first === undefined)
      throw new RingError("RING_DELEGATE_INVALID", { details: { reason: "inputs" } });
    const prepared = prepareDelegateTransfer({
      input,
      outputTreeId: input.outputTreeId ?? first.treeId,
    });
    checkWithdrawalAccounts(prepared, input.withdrawal?.accounts);
    const anonymous = isAnonymousFrame(railKind);
    // The member never signs, the transaction key seals the move to the auditor.
    const sealed = sealOutputs(
      {
        key:
          input.txViewingKey === undefined
            ? ViewingKey.generate()
            : ViewingKey.fromBytes(input.txViewingKey.slice() as Bytes32),
        prepared,
        delivery: () =>
          anonymous
            ? { kind: "anonymous", plan: delegatePlan(input.outputs) }
            : { kind: "delivered" },
        payments: [],
        salt: undefined,
        sender: input.source,
        assets: input.assets,
        auditor: config.auditorPublicKey,
        instructionDiscriminator: InstructionTag.ringAuthorityTransact,
        zeroChangeAsset: undefined,
      },
      (rail, proofInputs) => {
        checkCoOwnerKeys(proofInputs, input.source, coSources);
        return { rail, proofInputs };
      },
    );
    return proveSealed(
      sealed.rail,
      () => ({
        client,
        ringProgramId,
        config,
        staged: {
          proofInputs: sealed.proofInputs,
          rail: sealed.rail,
          sender: input.source,
          ownerSigners: [],
          ...(input.withdrawal === undefined ? {} : { withdrawal: input.withdrawal.accounts }),
          spend: {
            rail: railKind,
            delegate: input.delegate,
            ...(anonymous
              ? {
                  anonymous: {
                    partyIdentities: (coSigners: readonly Address[]) =>
                      anonymousPartyIdentities(coSigners),
                  },
                }
              : {}),
            prove: (inputs, proveContext) =>
              client.proveRingAuthorityTransact(
                inputs,
                ringProgramId,
                ringSpendAuthority({
                  keys: input.sourceProofs,
                  coOwners: coSources,
                  proofInputs: inputs,
                  sender: input.source,
                }),
                proveContext,
                client,
              ),
          },
        },
        ...approvedStage(approval, prepared),
      }),
      context,
    );
  }
}

/** Ring outputs in declared order, an exit carries no payment. */
function delegatePlan(outputs: readonly DelegateOutput[]): AnonymousPlan {
  return outputs
    .filter((output) => output.exit !== true)
    .reduce(
      (planned, output) =>
        planned.withPayment({ recipient: output.recipient.viewingPublicKey, reply: null }),
      AnonymousPlan.create(),
    );
}

/** SPP's authority builder takes no leg, the withdrawal holds a slot through its change sum and leaves padding. */
function prepareDelegateTransfer(
  params: Readonly<{ input: DelegateTransferInput; outputTreeId: number }>,
): PreparedTransfer {
  const { input, outputTreeId } = params;
  const { withdrawal } = input;
  const planned = {
    owner: input.source,
    inputs: input.inputs.map((spent) => standIn(spent, input.source)),
    payer: input.payer,
    ringProgramId: input.ringProgramId,
    outputTreeId,
  };
  const spendingReal = (prepared: PreparedTransfer): PreparedTransfer =>
    Object.freeze({ ...prepared, inputs: Object.freeze([...input.inputs]) });
  if (withdrawal === undefined)
    return spendingReal(prepareRingAuthorityTransfer({ ...planned, outputs: input.outputs }));
  const prepared = prepareRingAuthorityTransfer({
    ...planned,
    outputs: [
      ...input.outputs,
      { recipient: input.source, asset: withdrawal.asset, amount: withdrawal.amount },
    ],
  });
  const leg = prepared.outputs.length - 1;
  return spendingReal({
    ...prepared,
    interfaceTransfers: Object.freeze([settlementLeg(withdrawal)]),
    outputs: Object.freeze(prepared.outputs.slice(0, leg)),
    proofOutputs: () =>
      Object.freeze(
        prepared
          .proofOutputs()
          .map((output, slot) =>
            slot === leg ? delegatePadding(input.ringProgramId, output.blinding) : output,
          ),
      ),
  });
}

function delegatePadding(ringProgramId: Address, blinding: Bytes32): ProofOutputUtxo {
  return createProofOutput({ asset: SOL_MINT, amount: 0n, blinding, ringProgramId });
}

function settlementLeg(withdrawal: DelegateWithdrawal): SettlementTransfer {
  const { target, amount, asset } = withdrawal;
  return target.kind === "sol"
    ? { kind: "sol", isDeposit: false, amount, userSolAccount: target.recipient }
    : {
        kind: "spl",
        mint: asset,
        isDeposit: false,
        amount,
        tokenAccount: target.recipientTokenAccount,
        splInterfaceBump: target.splInterfaceBump,
      };
}

/** A Solana address names a wallet or a PDA alike, a key rebuilt as Ed25519 never equals a PDA owner. */
function ownedBy(spent: ProofInputUtxo, owner: Address): boolean {
  return equalBytes(spent.utxo.owner.confidentialViewTag(), addressBytes(owner));
}

export interface RingRelayInput {
  /** Names the fee payer the transfer pays from. */
  readonly quote: RelayQuote;
  /** Ring RPC proves with it and so links every spend of the owner. */
  readonly nullifierKey: NullifierKey;
  /** The wallet notes the transfer spends, each names its leaf. */
  readonly notes: readonly WalletUtxo[];
  /** Ring RPC creates each account before the withdrawal pays it. */
  readonly tokenAccountSetup?: readonly TokenAccountSetup[];
  /** The row a Ring RPC reseal refusal names, unset seals under the public counts' row. */
  readonly sealRow?: SealRow;
}

/** Rust `deposit_relay`'s inputs, the frame's sender proves nothing. */
export interface DepositRelayInput {
  readonly quote: RelayQuote;
  readonly nullifierKey: NullifierKey;
}

/** A sealed deposit frame, the caller wipes `payload`. */
export interface DepositRelayFrame {
  readonly payload: Uint8Array;
  readonly legs: readonly RelayLeg[];
  readonly expected: ExpectedSealed;
}

export interface PreparedRingRelay {
  readonly relay: UnsignedRelay;
}
