import { equalBytes, isZero } from "@heliuslabs/spr/internal";
import type { Bytes32 } from "@heliuslabs/spr/interface";
import type { ShieldedAddress, X25519PublicKey } from "@heliuslabs/spr/keypair";
import type { ProofOutputUtxo } from "@heliuslabs/spr/transaction";

import { bootstrapSlotTag, slotTag } from "../../crypto/derivation/ring-stream.js";
import type { ReplyMetadata } from "../codec/anonymous-delivery.js";
import { RingError } from "../error.js";

/** One recipient record under the tag its wallet selected. */
export interface PlannedPayment {
  /** The viewing key the record seals to. */
  readonly recipient: X25519PublicKey;
  /** `null` sends no reply, the caller always names the choice. */
  readonly reply: ReplyMetadata | null;
  /** At the width a ring slot publishes. */
  readonly tag: Bytes32;
}

/** One slot of an anonymous transfer, Rust `AnonymousSlot`. */
export type AnonymousSlot =
  /** At most two lead, each sealed to the sender. */
  | Readonly<{ kind: "change" }>
  | Readonly<{ kind: "payment"; payment: PlannedPayment }>
  | Readonly<{ kind: "exit" }>
  | Readonly<{ kind: "dummy" }>;

/** Payments in declared order, Rust `AnonymousPlan`. */
export class AnonymousPlan {
  readonly payments: readonly PlannedPayment[];
  /** One per change slot in slot order, a change slot past them takes random bytes. */
  readonly #changeTags: readonly Bytes32[];

  private constructor(payments: readonly PlannedPayment[], changeTags: readonly Bytes32[]) {
    this.payments = Object.freeze([...payments]);
    this.#changeTags = Object.freeze([...changeTags]);
    Object.freeze(this);
  }

  static create(): AnonymousPlan {
    return new AnonymousPlan([], []);
  }

  /** A relay's plan, the sealed outputs already publish the wallet's tags. */
  static fromRecipients(recipients: readonly X25519PublicKey[]): AnonymousPlan {
    return recipients.reduce(
      (plan, recipient) => plan.withPayment({ recipient, reply: null }),
      AnonymousPlan.create(),
    );
  }

  /** Under the recipient's bootstrap tag until `withTag` names another. */
  withPayment(payment: Omit<PlannedPayment, "tag">): AnonymousPlan {
    const tag = bootstrapSlotTag(payment.recipient.u());
    return new AnonymousPlan(
      [...this.payments, Object.freeze({ ...payment, tag })],
      this.#changeTags,
    );
  }

  /** Tags the latest payment. */
  withTag(tag: Bytes32): AnonymousPlan {
    return this.#taggingLatest(slotTag(tag));
  }

  /** No tag finds the latest payment, the note its recipient spends in the same frame does. */
  withRandomTag(): AnonymousPlan {
    return this.#taggingLatest(randomSlotTag());
  }

  /** A tag one change slot publishes, each call names the next change slot. */
  withSenderTag(tag: Bytes32): AnonymousPlan {
    return new AnonymousPlan(this.payments, [...this.#changeTags, slotTag(tag)]);
  }

  /** Change leads, every other ring output takes the next payment in declared order. */
  slots(outputs: readonly ProofOutputUtxo[], sender: ShieldedAddress): readonly AnonymousSlot[] {
    const change = leadingChange(outputs, sender);
    const payments = this.payments[Symbol.iterator]();
    const slots = outputs.map((output, slotIndex): AnonymousSlot => {
      const owner = output.ownerAddress;
      if (slotIndex < change) return Object.freeze({ kind: "change" });
      if (owner === undefined) return Object.freeze({ kind: "dummy" });
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

  /**
   * The tag each slot publishes, `undefined` on an exit, its owner tag is no view tag.
   * A frame publishes each tag once and random bytes on a slot that would repeat one, a match
   * returns every slot of the transaction and equal tags would show two slots share an owner.
   */
  tags(slots: readonly AnonymousSlot[]): readonly (Bytes32 | undefined)[] {
    const published: Bytes32[] = [];
    const once = (tag: Bytes32): Bytes32 => {
      if (published.some((earlier) => equalBytes(earlier, tag))) return randomSlotTag();
      published.push(tag);
      return tag;
    };
    const changeTags = this.#changeTags[Symbol.iterator]();
    return slots.map((slot) => {
      switch (slot.kind) {
        case "exit":
          return undefined;
        case "change": {
          const next = changeTags.next();
          return next.done === true ? randomSlotTag() : once(next.value);
        }
        case "dummy":
          return randomSlotTag();
        case "payment":
          return once(slot.payment.tag);
      }
    });
  }

  #taggingLatest(tag: Bytes32): AnonymousPlan {
    const latest = this.payments.at(-1);
    if (latest === undefined) return this;
    return new AnonymousPlan(
      [...this.payments.slice(0, -1), Object.freeze({ ...latest, tag })],
      this.#changeTags,
    );
  }
}

/** A dummy's tag and an untagged change's, uniform as a live tag's published bytes are. */
function randomSlotTag(): Bytes32 {
  return slotTag(globalThis.crypto.getRandomValues(new Uint8Array(32)) as Bytes32);
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
function carriesData(output: ProofOutputUtxo): boolean {
  return (
    (output.dataHash !== undefined && !isZero(output.dataHash)) ||
    (output.ringDataHash !== undefined && !isZero(output.ringDataHash))
  );
}

function leadingChange(outputs: readonly ProofOutputUtxo[], sender: ShieldedAddress): number {
  const first = outputs.findIndex((output) => !isChange(output, sender));
  return first < 0 ? outputs.length : first;
}
