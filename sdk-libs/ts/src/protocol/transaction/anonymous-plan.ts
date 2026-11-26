import { equalBytes, isZero } from "@heliuslabs/spr/internal";
import type { Bytes32 } from "@heliuslabs/spr/interface";
import { SPP_SUPPORTED_SHAPES } from "@heliuslabs/spr/interface";
import type { ShieldedAddress, X25519PublicKey } from "@heliuslabs/spr/keypair";
import { TransactionError, type ProofOutputUtxo } from "@heliuslabs/spr/transaction";

import { randomTagPreimage } from "../../crypto/derivation/dummy-tag.js";
import type { ReplyMetadata } from "../codec/anonymous-delivery.js";
import { RingError } from "../error.js";

/** The dummy preimages a plan draws ahead. */
const MAX_OUTPUTS = Math.max(...SPP_SUPPORTED_SHAPES.map((shape) => shape.outputs));

/** One recipient record, its tag derived from the slot secret. */
export interface PlannedPayment {
  /** The viewing key the record seals to. */
  readonly recipient: X25519PublicKey;
  /** `null` sends no reply, the caller always names the choice. */
  readonly reply: ReplyMetadata | null;
}

/** One slot of an anonymous transfer, Rust `AnonymousSlot`. */
export type AnonymousSlot =
  /** At most two lead, the tag zero. */
  | Readonly<{ kind: "change" }>
  | Readonly<{ kind: "payment"; payment: PlannedPayment }>
  | Readonly<{ kind: "exit" }>
  | Readonly<{ kind: "dummy"; preimage: Bytes32 }>;

/** Payments in declared order. */
export class AnonymousPlan {
  readonly payments: readonly PlannedPayment[];
  /** Drawn ahead, every encoding of one dummy slot publishes one tag. */
  readonly #dummyPreimages: readonly Bytes32[];

  private constructor(payments: readonly PlannedPayment[], dummyPreimages: readonly Bytes32[]) {
    this.payments = Object.freeze([...payments]);
    this.#dummyPreimages = dummyPreimages;
    Object.freeze(this);
  }

  static create(): AnonymousPlan {
    return new AnonymousPlan(
      [],
      Object.freeze(Array.from({ length: MAX_OUTPUTS }, () => randomTagPreimage())),
    );
  }

  /** `dummyPreimages` exactly as a sealed plan drew them. */
  static fromDummyPreimages(
    payments: readonly PlannedPayment[],
    dummyPreimages: readonly Bytes32[],
  ): AnonymousPlan {
    if (dummyPreimages.length !== MAX_OUTPUTS)
      throw new TransactionError("TRANSACTION_TOO_MANY_OUTPUTS");
    return new AnonymousPlan(payments, Object.freeze([...dummyPreimages]));
  }

  /** One per output slot. */
  dummyPreimages(): readonly Bytes32[] {
    return this.#dummyPreimages;
  }

  withPayment(payment: PlannedPayment): AnonymousPlan {
    return new AnonymousPlan(
      [...this.payments, Object.freeze({ ...payment })],
      this.#dummyPreimages,
    );
  }

  /** Change leads, every other ring output takes the next payment in declared order. */
  slots(outputs: readonly ProofOutputUtxo[], sender: ShieldedAddress): readonly AnonymousSlot[] {
    const change = leadingChange(outputs, sender);
    const payments = this.payments[Symbol.iterator]();
    const slots = outputs.map((output, slotIndex): AnonymousSlot => {
      const owner = output.ownerAddress;
      const preimage = this.#dummyPreimages[slotIndex];
      if (preimage === undefined) throw new TransactionError("TRANSACTION_TOO_MANY_OUTPUTS");
      if (slotIndex < change) return Object.freeze({ kind: "change" });
      if (owner === undefined) return Object.freeze({ kind: "dummy", preimage });
      if (isChange(output, sender))
        throw new RingError("RING_ANONYMOUS_CHANGE", { details: { slotIndex } });
      if (output.ringProgramId === undefined) {
        if (owner.signingPublicKey.equals(sender.signingPublicKey))
          throw new RingError("RING_SENDER_EXIT", { details: { slotIndex } });
        return Object.freeze({ kind: "exit" });
      }
      const payment = payments.next();
      if (payment.done === true)
        throw new RingError("RING_UNPLANNED_OUTPUT", { details: { slotIndex } });
      if (!payment.value.recipient.equals(owner.viewingPublicKey))
        throw new RingError("RING_PAYMENT_SLOT", { details: { slotIndex } });
      return Object.freeze({ kind: "payment", payment: payment.value });
    });
    if (payments.next().done !== true)
      throw new RingError("RING_PAYMENT_SLOT", { details: { slotIndex: outputs.length } });
    return Object.freeze(slots);
  }
}

/** Ring change owned by the sender and carrying no data, it leads the anonymous frame. */
export function isChange(output: ProofOutputUtxo, sender: ShieldedAddress): boolean {
  return (
    output.ringProgramId !== undefined &&
    output.ownerAddress !== undefined &&
    !carriesData(output) &&
    equalBytes(output.ownerAddress.toBytes(), sender.toBytes())
  );
}

/** A nonzero data hash or ring data hash, the commitment covers both. */
export function carriesData(output: ProofOutputUtxo): boolean {
  return (
    (output.dataHash !== undefined && !isZero(output.dataHash)) ||
    (output.ringDataHash !== undefined && !isZero(output.ringDataHash))
  );
}

function leadingChange(outputs: readonly ProofOutputUtxo[], sender: ShieldedAddress): number {
  const first = outputs.findIndex((output) => !isChange(output, sender));
  return first < 0 ? outputs.length : first;
}
