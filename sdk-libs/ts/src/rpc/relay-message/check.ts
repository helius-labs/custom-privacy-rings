import {
  type AccountMeta,
  AccountRole,
  type Address,
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
import {
  TRANSACTION_SIZE_LIMIT,
  type Bytes32,
  type RequestContext,
  type Signature,
} from "@heliuslabs/spr/interface";
import { bootstrapSlotTag, txKeySlotSecret } from "../../crypto/derivation/ring-stream.js";
import {
  addressBytes,
  encodeBase58,
  equalBytes,
  isCanonicalField,
  isZero,
  LOADED_ACCOUNTS_DATA_SIZE_LIMIT,
  ZERO_32,
  ZERO_NULLIFIER_PK,
} from "@heliuslabs/spr/internal";
import { ShieldedPublicKey, type X25519PublicKey } from "@heliuslabs/spr/keypair";
import { Utxo } from "@heliuslabs/spr/transaction";
import { ringComputeUnitLimit } from "../../operations/management/admin.js";
import { openingTotals } from "../../protocol/membership/approval-totals.js";
import { memberOfAsset } from "../../protocol/membership/policy.js";
import { RingInstructionTag } from "../../protocol/transaction/instruction-tag.js";
import { isAnonymousFrame, RingTransactData } from "../../protocol/transaction/transact.js";
import {
  coSignerPairs,
  isDepositLeg,
  isSolLeg,
  knownPrefixEndingAt,
  mintAccountPosition,
  RING_TRANSACT_RING_CONFIG_INDEX,
  settlementLegs,
  transferLayout,
  transferRegistryCount,
  type AccountWindow,
  type SettlementLeg,
  type TransferLayout,
} from "../../protocol/transaction/transfer-layout.js";
import { RecordKind, type OutputRecord } from "../../protocol/codec/audit-stream.js";
import { RECIPIENT_KEY_LEN } from "../../protocol/codec/ring-disclosure-layout.js";
import {
  canonicalViewingKey,
  openExitRecord,
  openOutputSlot,
  openSealedDisclosure,
  type RingDisclosure,
} from "../../crypto/audit/ring-disclosure.js";
import { dummyUtxoHash } from "../../protocol/membership/utxo-hash.js";
import { x25519FromU } from "../../crypto/keys/x25519.js";
import { N_PUBLIC_SLOTS } from "../../protocol/transaction/spp.js";
import {
  MAX_TRANSACTION_ADDRESSES,
  MAX_TRANSACTION_INSTRUCTIONS,
  MAX_TRANSACTION_SIGNATURES,
} from "../../protocol/proof/transfer-shapes.js";
import {
  mismatch,
  type ExpectedRelayMessage,
  type ExpectedFrame,
  withMismatch,
  type PublicLeg,
  type ExpectedDelegate,
  type ExpectedOwner,
  type ExpectedPayment,
  type SlotOpening,
  type ExpectedPublicOwner,
  type ExpectedCoSigner,
  type ExactApproval,
  type ExpectedFunder,
  ZERO_ADDRESS,
} from "./expected.js";

import { mintInputSpends } from "./mint-inputs.js";

const base58 = getBase58Decoder();
const VERIFIED = Symbol("verified relay message");

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
      checkFunder(expected, frame, envelope, decoded);
      break;
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
      checkSoleSigner(envelope, decoded, slotsNaming(decoded, envelope.signer, layout.owners));
      break;
    }
  }
  const settlements = decoded.legs.map(({ transfer, accounts: { start } }) => {
    const asset = accounts[start + (mintAccountPosition(transfer) ?? 0)]?.address;
    const user = accounts[start + (isSolLeg(transfer) ? 1 : 3)]?.address;
    if (asset === undefined || user === undefined) throw mismatch("Accounts");
    return { asset, user };
  });
  const hash = withMismatch("Data", () =>
    RingTransactData.externalDataHash(
      frame,
      { auditorPubkey: expected.ringPolicy.auditorPublicKey.toBytes() },
      settlements,
    ),
  );
  if (!equalBytes(expected.terms.externalDataHash, hash)) throw mismatch("Data");
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
  const allowedSpends = expected.allowed?.checkSpends(envelope.realInputs, frame.nullifiers);
  const records = frameRecords(expected.txViewingKey, frame, recipientKeys);
  if (
    (decoded.layout.mint === undefined
      ? undefined
      : decoded.accounts[decoded.layout.mint]?.address) !== expected.mint
  )
    throw mismatch("Accounts");
  const spends =
    allowedSpends ?? mintInputSpends({ expected, frame, records, realInputs: envelope.realInputs });
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
  const anonymous = isAnonymousFrame(frame.rail);
  // An anonymous frame publishes each tag once, a match returns every slot of the transaction.
  const published = frame.outputs
    .filter((output) => output.body.kind !== "exit")
    .map((output) => output.ownerTag);
  const unpaid = [...expected.payments];
  records.forEach((record, slot) => {
    if (record.kind === RecordKind.dummy) return;
    if (expected.mint !== undefined && record.mint !== expected.mint) throw mismatch("Recipients");
    const output = frame.outputs[slot];
    const sealedTo = (owner: ExpectedOwner): boolean =>
      (record.kind === RecordKind.exit ||
        (recipientKeys[slot] !== undefined && equalBytes(recipientKeys[slot], owner.viewingU))) &&
      output !== undefined &&
      rebuilds(expected, output.hash, record, owner);
    const matches = (payment: ExpectedPayment): boolean =>
      record.kind === payment.kind &&
      owns(payment.owner, record) &&
      record.mint === payment.mint &&
      record.amount === payment.amount;
    // A payment no published tag finds is lost to its recipient's sync.
    const tagged = (payment: ExpectedPayment): boolean =>
      anonymous && payment.kind === RecordKind.ring
        ? published.some((tag) => equalBytes(tag, payment.tag))
        : output !== undefined && equalBytes(payment.tag, output.ownerTag);
    const paid = unpaid.findIndex(
      (payment) => matches(payment) && tagged(payment) && sealedTo(payment.owner),
    );
    if (paid >= 0) return swapRemove(unpaid, paid);
    const source =
      record.kind === RecordKind.ring
        ? expected.sources.find((owner) => owns(owner, record) && sealedTo(owner))
        : undefined;
    if (source === undefined) throw mismatch("Recipients");
    spends.debit(source, record.mint, record.amount);
    // An anonymous source finds its change by the note it spent.
    if (anonymous) return;
    // A confidential source finds its change by the tag.
    if (output === undefined || !equalBytes(output.ownerTag, bootstrapSlotTag(source.viewingU)))
      throw mismatch("Recipients");
  });
  if (unpaid.length !== 0) throw mismatch("Recipients");
  spends.checkChange();
}

/** Rust `swap_remove`, the order decides the equal payment a later record takes. */
function swapRemove<Item>(list: Item[], at: number): void {
  const last = list.pop();
  if (last !== undefined && at < list.length) list[at] = last;
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
 * SPP reads one signature as approval of every data-bearing output its signer owns, and a ring
 * output's published tag names no signer, so no frame field names those outputs. The signer
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
  if (terms.slots.some((opening, slot) => opening.slot !== slot)) throw mismatch("UnopenedOutput");
  // 2. Each opening rebuilds the commitment published at its slot, so the signer reads each owner,
  // amount and data hash the proof commits, and the tag a non-exit slot publishes. An exit commits
  // no ring, a ring id there would refuse every exit.
  checkPublished(frame, terms.slots, (opening, exit) =>
    openingHash(opening, exit ? undefined : terms.ring),
  );
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

/** One named slot per published output in slot order, each with its commitment and off an exit its tag. */
function checkPublished<Slot extends Readonly<{ tag?: Bytes32 }>>(
  frame: RingTransactData,
  slots: readonly Slot[],
  commitment: (slot: Slot, exit: boolean) => Bytes32,
): void {
  if (slots.length !== frame.outputs.length) throw mismatch("UnopenedOutput");
  slots.forEach((named, slot) => {
    const published = frame.outputs[slot];
    if (published === undefined) throw mismatch("Recipients");
    const exit = published.body.kind === "exit";
    const hash = withMismatch("Recipients", () => commitment(named, exit));
    const tagged = named.tag !== undefined && equalBytes(named.tag, published.ownerTag);
    if (!equalBytes(hash, published.hash) || (!exit && !tagged)) throw mismatch("Recipients");
  });
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

/** A relayed deposit's proof holds no owner signature, the funder's binds every output and leg. */
function checkFunder(
  expected: ExpectedFunder,
  frame: RingTransactData,
  envelope: Envelope,
  decoded: FrameAccounts,
): void {
  const { terms } = expected;
  // 1. Each published output is the one the sealing wallet stated at its slot.
  checkPublished(frame, terms.slots, (named) => named.hash);
  // 2. The frame moves public funds through the legs the terms name and no other, a withdrawal
  // has no funder to refuse it.
  checkLegs(terms.legs, decoded.legs, decoded.accounts);
  // 3. A run entry would authorize data outputs and spent notes the funder's terms never name.
  const funded = decoded.legs.filter(
    (leg) => leg.funder !== undefined && decoded.accounts[leg.funder]?.address === terms.signer,
  );
  // SPP debits the funder of a SOL leg, one write bit covers every slot of the key.
  checkSignerSlots(
    envelope,
    decoded,
    funded.flatMap((leg) => (leg.funder === undefined ? [] : [leg.funder])),
    funded.some((leg) => isSolLeg(leg.transfer)) ? "accepted" : "refused",
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
