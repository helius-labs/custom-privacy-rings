/** The ring output rails, mirrors Rust `zolana-ring-transaction` over the ring program's expansion. */
import { encryptConfidential, equalBytes, isZero, ZERO_32 } from "@heliuslabs/spr/internal";
import type { Address } from "@heliuslabs/spr/interface";
import type { Bytes16, Bytes32, MessageData, TransactOutput } from "@heliuslabs/spr/interface";
import { randomSalt, ViewingKey } from "@heliuslabs/spr/keypair";
import { ShieldedAddress } from "@heliuslabs/spr/keypair";
import type { X25519PublicKey } from "@heliuslabs/spr/keypair";
import {
  createExternalData,
  createProofOutput,
  ProofInputUtxo,
  SOL_MINT,
  SppProofInputs,
  TransactionError,
  type AssetRegistry,
  type ConfidentialTransfer,
  type PreparedTransfer,
  type ProofOutputUtxo,
} from "@heliuslabs/spr/transaction";

import {
  encodeRingOutput,
  readRingOutput,
  ringBodyOutput,
  RingScheme,
  senderBundleOutputBody,
} from "../../protocol/transaction/output.js";
import {
  PAYLOAD_LENGTH_LEN,
  publishedRecordLen,
  type RecordForm,
} from "../../protocol/codec/ring-disclosure-layout.js";
import {
  replyLength,
  sealReply,
  sealSenderBundle,
} from "../../protocol/codec/anonymous-delivery.js";
import {
  DUMMY_RECORD,
  RecordKind,
  sealedRecordKey,
  encodeRecipientPayload,
  type OutputRecord,
  type RecipientPayload,
} from "../../protocol/codec/audit-stream.js";
import { sealOutputSlot } from "../../crypto/audit/ring-disclosure.js";
import {
  anonymousSlotTag,
  bundleTag,
  slotTag,
  txKeySlotSecret,
} from "../../crypto/derivation/ring-stream.js";
import { dummyRecipientKey } from "../../crypto/derivation/dummy-key.js";
import { withOutputFields } from "../../protocol/proof/proof-output.js";
import type { AnonymousPayment } from "./anonymous.js";
import {
  AnonymousPlan,
  isChange,
  type AnonymousSlot,
} from "../../protocol/transaction/anonymous-plan.js";
import { RingAddress } from "../../crypto/keys/ring-address.js";
import {
  OutputBlindings,
  ringBlindedOutputs,
  ringBlindingSeed,
} from "../../crypto/derivation/blinding.js";
import { RingError } from "../../protocol/error.js";
import {
  ringTransactOutput,
  type PublishedSenderBundle,
  type RingOutput,
} from "../../protocol/transaction/transact.js";

/** How the ring outputs of one transfer reach their recipients, Rust `RingDelivery`. */
export type RingDelivery =
  /** Each record under the owner's confidential tag. */
  | Readonly<{ kind: "delivered" }>
  /** Each record under a view tag derived from its slot secret, sender bundle in a message. */
  | Readonly<{ kind: "anonymous"; plan: AnonymousPlan }>;

/** Zero on change, `undefined` on an exit whose scheme 3 owner tag stays on the frame, Rust `RingOutputRail::anonymous_tag`. */
export function anonymousTag(
  slot: AnonymousSlot,
  slotSecret: (recipient: X25519PublicKey) => Bytes32,
): Bytes32 | undefined {
  switch (slot.kind) {
    case "exit":
      return undefined;
    case "change":
      return ZERO_32;
    case "dummy":
      return anonymousSlotTag(slot.preimage);
    case "payment": {
      const secret = slotSecret(slot.payment.recipient);
      try {
        return anonymousSlotTag(secret);
      } finally {
        secret.fill(0);
      }
    }
  }
}

/** The change at `added` leads, SPL ahead of SOL, then each dataless output declared to the sender, Rust `lead_change`. */
export function leadChange(
  outputs: readonly ProofOutputUtxo[],
  sender: ShieldedAddress,
  added: ReadonlySet<number>,
): readonly ProofOutputUtxo[] {
  const rank = (output: ProofOutputUtxo, index: number): number => {
    if (added.has(index)) return output.asset === SOL_MINT ? 1 : 0;
    return isChange(output, sender) ? 2 : 3;
  };
  return Object.freeze(
    outputs
      .map((output, index) => ({ output, index, rank: rank(output, index) }))
      .sort((left, right) => left.rank - right.rank || left.index - right.index)
      .map(({ output }) => output),
  );
}

/** The first padding slot of a ring frame, where a zero change goes when nothing returns, `-1` with none spare. */
export function spareSlot(outputs: readonly ProofOutputUtxo[]): number {
  return outputs.findIndex((output) => output.isDummy() && output.ringProgramId !== undefined);
}

/** The application data `output` sends beside its record, Rust `recipient_payload`. */
export function recipientPayload(output: ProofOutputUtxo): RecipientPayload {
  return Object.freeze({ data: output.data.utxoData() ?? new Uint8Array() });
}

/** The circuit derives the kind and the zero-key bit from the slot's opening, a dummy's plaintext is zero, Rust `RingOutputRail::output_record`. */
export function outputRecord(output: ProofOutputUtxo): OutputRecord {
  const owner = output.ownerAddress;
  if (owner === undefined || output.isDummy()) return DUMMY_RECORD;
  const kind = output.ringProgramId === undefined ? RecordKind.exit : RecordKind.ring;
  return Object.freeze({
    kind,
    amount: output.amount,
    ownerKey: owner.signingPublicKey.confidentialViewTag() as Bytes32,
    mint: output.asset as Address,
    blinding: output.blinding,
    key: sealedRecordKey(kind, owner.nullifierPublicKey),
    dataHash: output.dataHash ?? ZERO_32,
    ringDataHash: output.ringDataHash ?? ZERO_32,
  });
}

/** Every check runs ahead of the one write, `payment` then holds an address object its output keeps as its owner. */
export function declarePayment(
  transfer: ConfidentialTransfer,
  payment: AnonymousPayment,
): AnonymousPayment {
  const { ringProgramId, listSalt } = payment.to;
  const recipient = ShieldedAddress.fromBytes(payment.to.recipient.toBytes());
  const declared = Object.freeze({
    ...payment,
    to: new RingAddress({ recipient, ringProgramId, listSalt }),
  });
  checkDataBound(payment.data);
  transfer.sendToRing(recipient, payment.asset, payment.amount, ringProgramId);
  return declared;
}

/** SPP commits the application hash unchecked, a zero one binds no data. */
function checkDataBound(data: AnonymousPayment["data"], slotIndex?: number): void {
  if (data?.application !== undefined && isZero(data.application.hash))
    throw new RingError("RING_NOTE_DATA_UNBOUND", {
      details: slotIndex === undefined ? {} : { slotIndex },
    });
}

/** Each payment's data goes to the output that holds its owner object, whatever slot the frame gives it. */
function withPaymentData(
  outputs: readonly ProofOutputUtxo[],
  payments: readonly Pick<AnonymousPayment, "to" | "data">[],
): readonly ProofOutputUtxo[] {
  const bound = outputs.map((output, slot) => {
    const data = payments.find((payment) => payment.to.recipient === output.ownerAddress)?.data;
    if (data === undefined) return output;
    checkDataBound(data, slot);
    const { ringDataHash, application } = data;
    const hashed = ringDataHash === undefined ? output : withOutputFields(output, { ringDataHash });
    return application === undefined
      ? hashed
      : hashed.withUtxoData(application.bytes, application.hash);
  });
  const lost = payments.findIndex(
    (payment) =>
      payment.data !== undefined &&
      !outputs.some((output) => output.ownerAddress === payment.to.recipient),
  );
  if (lost >= 0) throw framingError(lost);
  return bound;
}

/** What every slot of one frame shares, a length of its own would mark a data note, Rust `BodyForm`. */
export interface BodyForm {
  readonly records: RecordForm;
  /** Every recipient payload's length, set exactly under `records.data`. */
  readonly payloadLength: number | undefined;
}

/**
 * `data` under a nonzero `dataHash` or application data, `ringData` under a nonzero `ringDataHash`,
 * an exit under a ring data hash refused, Rust `BodyForm::of`.
 */
export function bodyForm(outputs: readonly ProofOutputUtxo[]): BodyForm {
  const published = publishedOutputs(outputs);
  const nonzero = (hash: Bytes32 | undefined): boolean => hash !== undefined && !isZero(hash);
  const exitRingData = published.findIndex(
    (output) =>
      !output.isDummy() && output.ringProgramId === undefined && nonzero(output.ringDataHash),
  );
  if (exitRingData >= 0)
    throw new RingError("RING_EXIT_RING_DATA", { details: { slotIndex: exitRingData } });
  const live = published.filter((output) => !output.isDummy());
  const records: RecordForm = Object.freeze({
    data: live.some((output) => nonzero(output.dataHash) || output.data.utxoData() !== undefined),
    ringData: live.some((output) => nonzero(output.ringDataHash)),
  });
  return Object.freeze({
    records,
    payloadLength: records.data
      ? published.reduce(
          (width, output) =>
            output.ringProgramId === undefined && !output.isDummy()
              ? width
              : Math.max(width, encodeRecipientPayload(recipientPayload(output)).length),
          PAYLOAD_LENGTH_LEN,
        )
      : undefined,
  });
}

export type SlotWire = RingOutput;

/** One published slot, its compact wire form and the key its record seals to, Rust `PublishedSlot`. */
export interface PublishedSlot {
  readonly output: ProofOutputUtxo;
  /** The final slot index of `output`. */
  readonly slotIndex: number;
  readonly wire: SlotWire;
  /** `V_i`, sealed to the auditor beside the envelope. */
  readonly recipientKey: X25519PublicKey;
  /** The slot's reply ciphertext when the frame carries replies. */
  readonly reply?: Uint8Array;
  /** Set on the anonymous rail. */
  readonly anonymous?: AnonymousSlot;
}

/** The frame-wide form and lengths every slot of one transfer shares. */
type FrameLengths = BodyForm &
  Readonly<{
    /** `undefined` when no payment names a reply. */
    replyLength: number | undefined;
  }>;

/** One slot before framing, its key and tag fixed by the delivery and the slot index alone. */
type PlannedSlot = Readonly<{
  output: ProofOutputUtxo;
  recipientKey: X25519PublicKey;
  /** `undefined` on an exit, whose tag is its owner's. */
  tag: Bytes32 | undefined;
  anonymous?: AnonymousSlot;
}>;

/**
 * Rust `RingOutputRail`, keyed by a transaction viewing key the caller destroys, a rebuild draws a
 * new salt so a slot secret never repeats.
 */
export class RingOutputRail {
  readonly #payments: readonly Pick<AnonymousPayment, "to" | "data">[];
  readonly txViewingKey: ViewingKey;
  readonly salt: Bytes16;
  readonly delivery: RingDelivery;
  readonly #firstNullifier: Bytes32;

  /** `firstNullifier` is the transfer's input slot 0, padding never moves it, a program commitment fixes `salt`. */
  constructor(
    txViewingKey: ViewingKey,
    firstNullifier: Bytes32,
    delivery: RingDelivery,
    payments: readonly Pick<AnonymousPayment, "to" | "data">[] = [],
    salt: Bytes16 = randomSalt(),
  ) {
    this.#payments = payments;
    this.txViewingKey = txViewingKey;
    this.salt = salt;
    this.delivery = delivery;
    this.#firstNullifier = firstNullifier;
    Object.freeze(this);
  }

  /** The CRDS slot secret of the slot at `slotIndex` sealed to `recipient`. */
  slotSecret(recipient: X25519PublicKey, slotIndex: number): Bytes32 {
    return txKeySlotSecret({
      txViewingKey: this.txViewingKey,
      recipientViewingKey: recipient,
      salt: this.salt,
      outputIndex: slotIndex,
      firstNullifier: this.#firstNullifier,
    });
  }

  /** SPP's `blinding_seed`, rebuilt by any holder of the transaction key. */
  blindingSeed(): Bytes32 {
    const txViewingSecret = this.txViewingKey.secretBytes();
    try {
      return ringBlindingSeed(txViewingSecret, this.salt);
    } finally {
      txViewingSecret.fill(0);
    }
  }

  /** Seals every slot of `prepared` under its own inline tag, Rust `RingOutputRail::encrypt`. */
  encrypt(
    input: Readonly<{
      prepared: PreparedTransfer;
      sender: ShieldedAddress;
      assets: AssetRegistry;
      /** An exit's record seals to the auditor's key. */
      auditor: X25519PublicKey;
      instructionDiscriminator: number;
      /** Set under a program commitment, its circuit keeps a zero change in the asset it spent. */
      zeroChangeAsset?: Address;
    }>,
  ): SppProofInputs {
    const { prepared, sender, auditor } = input;
    if (!equalBytes(prepared.firstNullifier, this.#firstNullifier)) throw framingError(0);
    const anonymous = this.delivery.kind === "anonymous";
    const proofOutputs = withPaymentData(prepared.proofOutputs(), this.#payments);
    checkCompactSuffix(proofOutputs);
    // SPP leads with the change it adds, an anonymous frame with no change of the sender takes a zero one.
    const added = new Set(Array.from({ length: prepared.senderOutputCount }, (_, slot) => slot));
    const spare =
      anonymous && !proofOutputs.some((output) => isChange(output, sender))
        ? spareSlot(proofOutputs)
        : -1;
    const ringProgramId = proofOutputs[spare]?.ringProgramId;
    const zeroChange =
      ringProgramId === undefined
        ? undefined
        : createProofOutput({
            ownerAddress: sender,
            asset: input.zeroChangeAsset ?? SOL_MINT,
            amount: 0n,
            ringProgramId,
          });
    if (zeroChange !== undefined) added.add(spare);
    const led = leadChange(
      proofOutputs.map((output, slot) => (slot === spare ? (zeroChange ?? output) : output)),
      sender,
      added,
    );
    const planned = anonymous
      ? led.map((output) =>
          // The anonymous circuit folds the zero tag into every padding owner.
          output.isDummy()
            ? withOutputFields(output, { ownerTag: new Uint8Array(32) as Bytes32 })
            : output,
        )
      : led;
    const outputs = ringBlindedOutputs({
      outputs: planned,
      firstNullifier: this.#firstNullifier,
      blindingSeed: this.blindingSeed(),
    });
    const live = publishedOutputs(outputs);
    const slots = this.#plannedSlots(live, sender, auditor);
    const lengths = frameLengths(this.delivery, live);
    const messages: MessageData[] =
      this.delivery.kind === "anonymous"
        ? [
            {
              viewTag: this.#bundleTag(sender.viewingPublicKey, live.length),
              data: encodeRingOutput(
                RingScheme.ringSenderBundle,
                this.#senderBundle(slots.length, sender.viewingPublicKey),
              ),
            },
          ]
        : [];
    const published: TransactOutput[] = slots.map((slot, slotIndex) => {
      const wire = this.#wire(slot, slotIndex, prepared.outputTreeId, input.assets, lengths);
      return ringTransactOutput(wire, this.#reply(slot, slotIndex, lengths));
    });
    return new SppProofInputs({
      payer: prepared.payer,
      inputUtxos: paddedInputs(prepared),
      outputs,
      externalData: createExternalData({
        instructionDiscriminator: input.instructionDiscriminator,
        interfaceTransfers: prepared.interfaceTransfers,
        txViewingPublicKey: this.txViewingKey.publicKey(),
        salt: this.salt,
        outputs: published,
        resolvedOwnerTags: published.map(({ ownerTag }) =>
          ownerTag.kind === "inline" ? ownerTag.value : ZERO_32,
        ),
        messages,
      }),
      blindingSeed: this.blindingSeed(),
      outputTreeId: prepared.outputTreeId,
    });
  }

  /**
   * Every published slot in order, each expanding to exactly the output the SPP proof binds, Rust
   * `RingOutputRail::published_slots`.
   */
  publishedSlots(
    proofInputs: SppProofInputs,
    sender: ShieldedAddress,
    auditor: X25519PublicKey,
  ): readonly PublishedSlot[] {
    const outputs = publishedOutputs(proofInputs.outputs);
    const published = proofInputs.externalData.outputs;
    if (published.length !== outputs.length)
      throw framingError(Math.min(outputs.length, published.length));
    const slots = this.#plannedSlots(outputs, sender, auditor);
    const lengths: FrameLengths = Object.freeze({ ...bodyForm(outputs), replyLength: undefined });
    return Object.freeze(
      slots.map((slot, slotIndex) => {
        const expected = published[slotIndex];
        if (expected === undefined) throw framingError(slotIndex);
        // The exit body is scheme 3 under a random ephemeral, the published bytes are the wire's.
        const wire =
          slot.tag === undefined
            ? this.#exitWire(slot, slotIndex, expected, lengths.records)
            : this.#wire(slot, slotIndex, proofInputs.outputTreeId, undefined, lengths);
        const reply = replyOf(slot, expected, slotIndex);
        if (!sameOutput(ringTransactOutput(wire, reply), expected)) throw framingError(slotIndex);
        return Object.freeze({
          output: slot.output,
          slotIndex,
          wire: Object.freeze(wire),
          recipientKey: slot.recipientKey,
          ...(reply === undefined ? {} : { reply }),
          ...(slot.anonymous === undefined ? {} : { anonymous: slot.anonymous }),
        });
      }),
    );
  }

  /** The first message, its tag derived for index `m`. */
  publishedSender(
    proofInputs: SppProofInputs,
    senderViewingPublicKey: X25519PublicKey,
  ): PublishedSenderBundle | undefined {
    if (this.delivery.kind !== "anonymous") return undefined;
    const message = proofInputs.externalData.messages[0];
    const body = message === undefined ? undefined : senderBundleOutputBody(message.data);
    if (
      message === undefined ||
      body === undefined ||
      !equalBytes(
        message.viewTag,
        this.#bundleTag(senderViewingPublicKey, publishedOutputs(proofInputs.outputs).length),
      )
    )
      throw new RingError("RING_SENDER_BUNDLE_FRAMING");
    return Object.freeze({ tag: message.viewTag, body });
  }

  /** `V_i` and the tag of each slot, Rust `RingOutputRail::slot_keys`. */
  #plannedSlots(
    outputs: readonly ProofOutputUtxo[],
    sender: ShieldedAddress,
    auditor: X25519PublicKey,
  ): readonly PlannedSlot[] {
    // Every encoding of one dummy slot seals to one key, a relay holding the transaction key
    // rederives it.
    const dummyKey = (slotIndex: number): X25519PublicKey => {
      const txViewingSecret = this.txViewingKey.secretBytes();
      try {
        return dummyRecipientKey(txViewingSecret, slotIndex);
      } finally {
        txViewingSecret.fill(0);
      }
    };
    if (this.delivery.kind === "anonymous") {
      const anonymousSlots = this.delivery.plan.slots(outputs, sender);
      return anonymousSlots.map((slot, slotIndex) => {
        const output = outputs[slotIndex];
        if (output === undefined) throw framingError(slotIndex);
        const recipientKey =
          slot.kind === "payment"
            ? slot.payment.recipient
            : slot.kind === "change"
              ? sender.viewingPublicKey
              : slot.kind === "exit"
                ? auditor
                : dummyKey(slotIndex);
        const tag = anonymousTag(slot, (recipient) => this.slotSecret(recipient, slotIndex));
        return Object.freeze({ output, recipientKey, tag, anonymous: slot });
      });
    }
    return outputs.map((output, slotIndex) => {
      const owner = output.ownerAddress;
      if (owner === undefined || output.isDummy()) {
        const recipientKey = dummyKey(slotIndex);
        // The derived key publishes its confidential tag as a real recipient's would.
        return Object.freeze({
          output,
          recipientKey,
          tag: slotTag(recipientKey.u()),
        });
      }
      if (output.ringProgramId === undefined)
        return Object.freeze({ output, recipientKey: auditor, tag: undefined });
      return Object.freeze({
        output,
        recipientKey: owner.viewingPublicKey,
        tag: slotTag(owner.viewingPublicKey.u()),
      });
    });
  }

  /** `record_ct` under the slot secret, then the payload or the exit's scheme 3 body. */
  #wire(
    slot: PlannedSlot,
    slotIndex: number,
    treeId: PreparedTransfer["outputTreeId"],
    assets: AssetRegistry | undefined,
    lengths: FrameLengths,
  ): SlotWire {
    const { output } = slot;
    const hash = output.hash(treeId);
    const secret = this.slotSecret(slot.recipientKey, slotIndex);
    try {
      const record = outputRecord(output);
      const form = lengths.records;
      if (slot.tag === undefined) {
        if (assets === undefined) throw framingError(slotIndex);
        return {
          hash,
          ownerTag: exitTag(output, slotIndex),
          record: sealOutputSlot(secret, { record, form }),
          body: { kind: "exit", payload: this.#exitBody(output, slotIndex, assets) },
        };
      }
      const sealed = sealOutputSlot(secret, {
        record,
        form,
        ...(lengths.payloadLength === undefined
          ? {}
          : { payload: recipientPayload(output), payloadLength: lengths.payloadLength }),
      });
      const recordLength = publishedRecordLen(form, RecordKind.ring);
      return {
        hash,
        ownerTag: slot.tag,
        record: sealed.slice(0, recordLength),
        body: { kind: "recipient", payload: sealed.slice(recordLength) },
      };
    } finally {
      secret.fill(0);
    }
  }

  /** The exit's scheme 3 body as published, its record resealed to check the frame. */
  #exitWire(
    slot: PlannedSlot,
    slotIndex: number,
    published: TransactOutput,
    form: RecordForm,
  ): SlotWire {
    const { hash, ownerTag } = inline(published, slotIndex);
    if (!equalBytes(ownerTag, exitTag(slot.output, slotIndex))) throw framingError(slotIndex);
    const secret = this.slotSecret(slot.recipientKey, slotIndex);
    try {
      return {
        hash,
        ownerTag,
        record: sealOutputSlot(secret, { record: outputRecord(slot.output), form }),
        body: {
          kind: "exit",
          payload: schemeBody(
            published.data ?? new Uint8Array(),
            RingScheme.confidential,
            slotIndex,
          ),
        },
      };
    } finally {
      secret.fill(0);
    }
  }

  /** Scheme 3 to the exit's owner at its slot, under the rail's salt. */
  #exitBody(output: ProofOutputUtxo, slotIndex: number, assets: AssetRegistry): Uint8Array {
    const owner = output.ownerAddress;
    if (owner === undefined) throw framingError(slotIndex);
    return encryptConfidential(
      this.txViewingKey,
      owner.viewingPublicKey,
      {
        assetId: assets.assetId(output.asset),
        amount: output.amount,
        blinding: output.blinding,
        data: output.data,
      },
      this.salt,
      slotIndex,
    );
  }

  /** Random bytes on a non-exit slot that names none, every reply shares one length. */
  #reply(slot: PlannedSlot, slotIndex: number, lengths: FrameLengths): Uint8Array | undefined {
    const { replyLength: length } = lengths;
    if (length === undefined || slot.tag === undefined) return undefined;
    const reply = slot.anonymous?.kind === "payment" ? slot.anonymous.payment.reply : null;
    if (reply === null) return randomBytes(length);
    const secret = this.slotSecret(slot.recipientKey, slotIndex);
    try {
      return sealReply(secret, reply, length);
    } finally {
      secret.fill(0);
    }
  }

  #bundleTag(senderViewingPublicKey: X25519PublicKey, outputCount: number): Bytes32 {
    const secret = this.slotSecret(senderViewingPublicKey, outputCount);
    try {
      return bundleTag(secret);
    } finally {
      secret.fill(0);
    }
  }

  /**
   * The output seed sealed to the sender at index `m`. The sender recovers its outputs through the
   * auditor stream, `V_k` of each slot there and change by trial under its own viewing keys.
   */
  #senderBundle(outputCount: number, senderKey: X25519PublicKey): Uint8Array {
    const blindings = new OutputBlindings(this.#firstNullifier, this.blindingSeed());
    const outputSeed = blindings.outputSeed();
    blindings.destroy();
    const secret = this.slotSecret(senderKey, outputCount);
    try {
      return sealSenderBundle(secret, { outputSeed });
    } finally {
      outputSeed.fill(0);
      secret.fill(0);
    }
  }
}
/** The reply bytes past a non-exit record, `undefined` on an exit or a frame without replies. */
function replyOf(
  slot: PlannedSlot,
  published: TransactOutput,
  slotIndex: number,
): Uint8Array | undefined {
  if (slot.tag === undefined) return undefined;
  const framedBody = ringBodyOutput(published.data ?? new Uint8Array());
  if (framedBody === undefined) throw framingError(slotIndex);
  return framedBody.reply.length === 0 ? undefined : framedBody.reply;
}

function schemeBody(data: Uint8Array, scheme: RingScheme, slotIndex: number): Uint8Array {
  const frame = readRingOutput(data);
  if (frame.encoding !== "encrypted" || frame.scheme !== scheme) throw framingError(slotIndex);
  return frame.body;
}

/** Omission removes a suffix, no index moves and every published slot keeps its blinding. */
function checkCompactSuffix(outputs: readonly ProofOutputUtxo[]): void {
  const first = outputs.findIndex((output) => output.isCompact());
  if (first >= 0 && outputs.slice(first).some((output) => !output.isCompact()))
    throw new RingError("RING_COMPACT_NOT_SUFFIX", { details: { slotIndex: first } });
}

/** `m` slots, a compact suffix leaves the frame. */
export function publishedOutputs(outputs: readonly ProofOutputUtxo[]): readonly ProofOutputUtxo[] {
  return outputs.filter((output) => !output.isCompact());
}

function inline(
  published: TransactOutput,
  slotIndex: number,
): Readonly<{ hash: Bytes32; ownerTag: Bytes32 }> {
  if (published.ownerTag.kind !== "inline") throw framingError(slotIndex);
  return { hash: published.utxoHash, ownerTag: published.ownerTag.value };
}

function sameOutput(expanded: TransactOutput, published: TransactOutput): boolean {
  return (
    equalBytes(expanded.utxoHash, published.utxoHash) &&
    expanded.ownerTag.kind === "inline" &&
    published.ownerTag.kind === "inline" &&
    equalBytes(expanded.ownerTag.value, published.ownerTag.value) &&
    equalBytes(expanded.data ?? new Uint8Array(), published.data ?? new Uint8Array())
  );
}

/** Random dummies join the last input tree and compact padding the first, as SPP reads them. */
function paddedInputs(prepared: PreparedTransfer): readonly ProofInputUtxo[] {
  const treeId = prepared.compactPadding ? prepared.inputTreeIds[0] : prepared.inputTreeIds.at(-1);
  if (treeId === undefined) throw new TransactionError("TRANSACTION_NO_INPUTS");
  const inputs = [...prepared.inputs];
  while (inputs.length < prepared.shape.inputs)
    inputs.push(
      prepared.compactPadding
        ? ProofInputUtxo.compact(treeId)
        : ProofInputUtxo.dummy(undefined, treeId),
    );
  return inputs;
}

/** The payload and reply lengths a sealer fixes, one per frame. */
function frameLengths(delivery: RingDelivery, outputs: readonly ProofOutputUtxo[]): FrameLengths {
  const replies =
    delivery.kind === "anonymous"
      ? delivery.plan.payments.flatMap((payment) => (payment.reply === null ? [] : [payment.reply]))
      : [];
  return Object.freeze({
    ...bodyForm(outputs),
    replyLength: replies.length === 0 ? undefined : replyLength(replies),
  });
}

/** SPP binds an exit's tag to its owner. */
function exitTag(output: ProofOutputUtxo, slotIndex: number): Bytes32 {
  const owner = output.ownerAddress;
  if (owner === undefined) throw framingError(slotIndex);
  return owner.signingPublicKey.confidentialViewTag() as Bytes32;
}

function framingError(slotIndex: number): RingError {
  return new RingError("RING_OUTPUT_FRAMING", { details: { slotIndex } });
}

function randomBytes(length: number): Uint8Array {
  const bytes = new Uint8Array(length);
  globalThis.crypto.getRandomValues(bytes);
  return bytes;
}
