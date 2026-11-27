import { bytesToHex } from "@noble/hashes/utils.js";
import { getAddressDecoder, getAddressEncoder } from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { initializePoseidon } from "@heliuslabs/spr";
import type { Bytes16, Bytes32 } from "@heliuslabs/spr/interface";
import { ShieldedKeypair, SigningKey } from "@heliuslabs/spr/keypair";
import { ShieldedPda } from "@heliuslabs/spr/program";
import {
  AssetRegistry,
  ConfidentialTransfer,
  ProofInputUtxo,
  Utxo,
  type ProofOutputUtxo,
} from "@heliuslabs/spr/transaction";

import { RingAddress } from "../../../src/crypto/keys/ring-address.js";
import { CustomRingTransfer } from "../../../src/operations/payments/transfer-rails.js";
import plans from "../../../../../test-vectors/proof/ring-transfer-plans.json" with { type: "json" };
import { filled, pdaKeys, ringFixture, walletKeys } from "../../helpers/ring-pipeline.js";

const bytes = (encoded: string) => Uint8Array.from(Buffer.from(encoded, "hex"));
const address = (encoded: string) => getAddressDecoder().decode(bytes(encoded));
const ring = address(plans.ring);
const assets = new AssetRegistry();

/** Owner, mint, amount, ring and data hash per slot, a dummy by its blinding alone. */
const slots = (outputs: readonly ProofOutputUtxo[], blinded: boolean) =>
  outputs.map((output) => ({
    owner: output.ownerAddress === undefined ? null : bytesToHex(output.ownerAddress.toBytes()),
    ...(output.ownerAddress === undefined
      ? {}
      : {
          mint: bytesToHex(new Uint8Array(getAddressEncoder().encode(output.asset))),
          amount: Number(output.amount),
          ring: output.ringProgramId !== undefined,
          dataHash:
            output.dataHash === undefined || output.dataHash.every((byte) => byte === 0)
              ? null
              : bytesToHex(output.dataHash),
        }),
    ...(blinded ? { blinding: bytesToHex(output.blinding) } : {}),
  }));

// Rust `every_pinned_transfer_seals_to_the_plan_both_sdks_share` writes the file.
describe("shared transfer plans", () => {
  beforeAll(initializePoseidon);

  it.each(plans.cases)("$name", async (pinned) => {
    const fixture = await ringFixture({
      mode: pinned.mode as "confidential" | "anonymous",
      ringProgramId: ring,
      authority: address(plans.payer),
    });
    const keypair = (seed: number) =>
      ShieldedKeypair.fromKeypair(SigningKey.fromEd25519Bytes(filled(seed)));
    const wallet = keypair(plans.ownerSeed);
    const holder = ShieldedPda.withViewingKey(
      address(plans.pda),
      wallet.nullifierKey(),
      wallet.viewingKey(),
    );
    const owner = pinned.pdaOwner ? holder : wallet;
    const sender = owner.shieldedAddress();
    fixture.enroll(sender);
    const notes = pinned.inputs.map((note) =>
      ProofInputUtxo.fromKeypair(
        new Utxo({
          owner: owner.signingPublicKey(),
          asset: address(note.mint),
          amount: BigInt(note.amount),
          blinding: bytes(note.blinding) as Bytes32,
          ringProgramId: ring,
        }),
        wallet,
        {},
        plans.treeId,
      ),
    );
    const rail = () => {
      const transfer = pinned.compact
        ? ConfidentialTransfer.compact(sender, notes, fixture.authority)
        : new ConfidentialTransfer(sender, notes, fixture.authority);
      const start = CustomRingTransfer.new({
        ringProgramId: ring,
        keys: pinned.pdaOwner ? pdaKeys(holder) : walletKeys(wallet),
        transfer,
        assets,
      }).spendPublic(sender.solanaAddress());
      return pinned.declared.reduce((built, declared) => {
        const asset = address(declared.mint);
        const amount = BigInt(declared.amount);
        if (declared.kind === "own") {
          transfer.sendToRing(sender, asset, amount, ring);
          return built;
        }
        if (declared.kind === "state") {
          const to = new RingAddress({
            recipient: sender,
            ringProgramId: ring,
            listSalt: filled(7),
          });
          const application = {
            hash: bytes(plans.stateHash) as Bytes32,
            bytes: bytes(plans.state),
          };
          return built.withPayment({ to, asset, amount, reply: null, data: { application } });
        }
        const recipient = keypair(declared.seed ?? 0).shieldedAddress();
        fixture.enroll(recipient);
        if (declared.kind === "exit") {
          transfer.sendDefaultRing(recipient, asset, amount);
          return built;
        }
        const to = new RingAddress({ recipient, ringProgramId: ring, listSalt: filled(7) });
        return built.withPayment({ to, asset, amount, reply: null });
      }, start);
    };

    await rail()
      .withProgram({
        salt: bytes(plans.salt) as Bytes16,
        privateTxHash: bytes(pinned.privateTxHash) as Bytes32,
        transactionHash: bytes(pinned.transactionHash) as Bytes32,
      })
      .prove(fixture.client);
    const staged = fixture.sppCalls.at(-1)?.proofInputs;
    expect(slots(staged?.outputs ?? [], true)).toEqual(pinned.outputs);
    expect(bytesToHex(staged?.blindingSeed ?? new Uint8Array())).toBe(pinned.blindingSeed);
    expect(bytesToHex(staged?.firstNullifier() ?? new Uint8Array())).toBe(pinned.firstNullifier);

    await rail().prove(fixture.client);
    expect(slots(fixture.sppCalls.at(-1)?.proofInputs.outputs ?? [], false)).toEqual(
      pinned.directOutputs,
    );
  });
});
