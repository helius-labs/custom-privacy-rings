import { describe, expect, it } from "vitest";

import bodies from "../../../../../test-vectors/audit/anonymous_bodies.json" with { type: "json" };
import signerSet from "../../../../../test-vectors/audit/signer_set.json" with { type: "json" };
import debits from "../../../../../test-vectors/policy/debit.json" with { type: "json" };
import zeroNullifier from "../../../../../test-vectors/registry/zero_nullifier.json" with { type: "json" };
import { ZERO_32, ZERO_NULLIFIER_PK, bigIntBytes, bytesToBigInt } from "@heliuslabs/spr/internal";
import type { Bytes16, Bytes31, Bytes32 } from "@heliuslabs/spr/interface";
import {
  ViewingKey,
  X25519Keypair,
  poseidon,
  NullifierKey,
  ShieldedAddress,
  ShieldedKeypair,
} from "@heliuslabs/spr/keypair";
import { Data } from "@heliuslabs/spr/transaction";

import { deliveredSlotSecret } from "../../../src/crypto/derivation/ring-stream.js";
import { x25519FromU } from "../../../src/crypto/keys/x25519.js";
import {
  MIN_REPLY_LEN,
  openReply,
  sealReply,
} from "../../../src/protocol/codec/anonymous-delivery.js";
import { ownerDebits } from "../../../src/protocol/membership/owner-debits.js";
import { signerBinding } from "../../../src/protocol/proof/statement.js";
import { LocalRingKeys } from "../../../src/state/ring-keys.js";

function bytes<T extends Uint8Array>(value: string, length: number): T {
  const decoded = Uint8Array.from(Buffer.from(value, "hex"));
  if (decoded.length !== length) throw new RangeError(`expected ${String(length)} bytes`);
  return decoded as T;
}

function field(value: string): Bytes32 {
  return bytes<Bytes32>(value, 32);
}

function hex(value: Uint8Array): string {
  return Buffer.from(value).toString("hex");
}

describe("signer_set.json", () => {
  it.each(signerSet.cases.map((vector) => [vector.name, vector] as const))(
    "chains %s",
    (_, vector) => {
      const binding = signerBinding(vector.signers.map(field), vector.slots);
      expect(binding.count).toBe(vector.signers.length);
      expect(hex(binding.chain)).toBe(vector.signer_chain);
    },
  );

  it("refuses a run past the key's slots", () => {
    expect(() => signerBinding([field(signerSet.cases[2]?.signers[0] ?? "")], 0)).toThrow(
      expect.objectContaining({ code: "RING_INVALID_LENGTH" }),
    );
  });
});

describe("anonymous_bodies.json", () => {
  const recipientKey = bodies.recipient_keys[bodies.body_index] ?? "";
  const salt = bytes<Bytes16>(bodies.salt, 16);
  const firstNullifier = field(bodies.first_nullifier);

  it("reads the body's slot secret through the holder under the recipient's own key", async () => {
    const viewing = ViewingKey.fromBytes(field(bodies.recipient_viewing_sk));
    const nullifierKey = NullifierKey.fromSecret(new Uint8Array(31).fill(7) as Bytes31);
    const recipient = LocalRingKeys.fromKeys({
      address: ShieldedAddress.fromPublicKeys(
        ShieldedKeypair.generate().signingPublicKey(),
        nullifierKey.publicKey(),
        viewing.publicKey(),
      ),
      viewingKeys: [viewing],
      listRoot: viewing,
      nullifierKey,
    });
    const txViewing = X25519Keypair.fromBytes(field(bodies.tx_viewing_sk));
    const { viewingPublicKey } = recipient.address();
    expect(hex(viewingPublicKey.u())).toBe(recipientKey);
    const secrets = await recipient.ringDeliverySecrets([
      {
        viewingPublicKey,
        txViewingPublicKey: txViewing.publicKey(),
        salt,
        outputIndex: bodies.body_index,
        firstNullifier,
      },
    ]);
    expect(secrets.map(hex)).toEqual([bodies.body_slot_secret]);
    txViewing.destroy();
  });

  it("rebuilds the body's CRDS slot secret from the transaction key", () => {
    const txViewing = X25519Keypair.fromBytes(field(bodies.tx_viewing_sk));
    const recipientViewingKey = x25519FromU(field(recipientKey));
    expect(
      hex(
        deliveredSlotSecret({
          agreement: txViewing.ecdh(recipientViewingKey),
          txViewingKey: txViewing.publicKey(),
          recipientViewingKey,
          salt,
          outputIndex: bodies.body_index,
          firstNullifier,
        }),
      ),
    ).toBe(bodies.body_slot_secret);
    txViewing.destroy();
  });

  it("opens a reply under the slot's secret", () => {
    const secret = field(bodies.body_slot_secret);
    const viewingKey = x25519FromU(field(bodies.sender_viewing_key));
    const reply = {
      viewingKey,
      ownerKey: new Uint8Array(32).fill(1) as Bytes32,
      nullifierPk: new Uint8Array(32).fill(2) as Bytes32,
      data: new Data(),
    };
    const sealed = sealReply(secret, reply, MIN_REPLY_LEN);
    const opened = openReply(secret, sealed);
    expect(opened?.claimed.viewingKey.u()).toEqual(viewingKey.u());
    expect(opened?.claimed.ownerKey).toEqual(reply.ownerKey);
    expect(opened?.claimed.nullifierPk).toEqual(reply.nullifierPk);
  });
});

describe("debit.json", () => {
  const identity = (value: number): Bytes32 => bigIntBytes(BigInt(value), 32) as Bytes32;
  const slot = (row: Readonly<{ owner: number; asset: number; amount: number }>) => ({
    owner: identity(row.owner),
    asset: identity(row.asset),
    amount: BigInt(row.amount),
  });

  it.each(debits.cases.map((vector) => [vector.name, vector] as const))(
    "debits %s, a zero row absent",
    (_, vector) => {
      const rows = ownerDebits({
        senders: vector.debits.map((row) => identity(row.owner)),
        assets: vector.debits.map((row) => identity(row.asset)),
        inputs: vector.inputs.map(slot),
        outputs: vector.outputs.map((output) => ({ ...slot(output), ring: output.ring })),
        publicGross: [],
      });
      expect(
        rows.map((row) => [bytesToBigInt(row.owner), bytesToBigInt(row.asset), row.debit]),
      ).toEqual(
        vector.debits
          .filter((row) => row.debit > 0)
          .map((row) => [BigInt(row.owner), BigInt(row.asset), BigInt(row.debit)]),
      );
    },
  );
});

describe("zero_nullifier.json", () => {
  it("names the key of nullifier secret zero", () => {
    expect(hex(ZERO_NULLIFIER_PK)).toBe(zeroNullifier.nullifier_pk);
    expect(hex(poseidon([ZERO_32]))).toBe(zeroNullifier.nullifier_pk);
  });
});
