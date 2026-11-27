import {
  AccountRole,
  getBase58Decoder,
  getBase58Encoder,
  getBase64Encoder,
  type Address,
} from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { initializePoseidon } from "@heliuslabs/spr";
import type { Bytes32, Bytes64 } from "@heliuslabs/spr/interface";
import { ShieldedKeypair, SigningKey } from "@heliuslabs/spr/keypair";
import { AssetRegistry, SOL_MINT } from "@heliuslabs/spr/transaction";

import { slotTag } from "../../../src/crypto/derivation/ring-stream.js";
import { RingAddress, RingPaymentRequest } from "../../../src/crypto/keys/ring-address.js";
import {
  PreparedRingDeposit,
  type RingDepositClient,
  type RingDepositParams,
} from "../../../src/operations/payments/deposit.js";
import {
  decodeRelayProofInputs,
  depositRequestDigest,
} from "../../../src/protocol/ring-rpc/relay-proof-inputs.js";
import { PendingRelaySignature } from "../../../src/rpc/relay.js";
import { relayRequestAttestation } from "../../../src/rpc/ring-rpc.js";
import { RingInstructionTag } from "../../../src/protocol/transaction/instruction-tag.js";
import { AnonymousSync } from "../../../src/state/anonymous-sync.js";
import { ViewingHistory } from "../../../src/state/viewing-history.js";
import { BLOCKHASH } from "../../../../../zolana/sdk-libs/ts/test/helpers/clients.js";
import {
  PROOF_SERVICE,
  freshWalletTags,
  indexedFrame,
  randomAddress,
  ringFixture,
  walletKeys,
  type Fixture,
} from "../../helpers/ring-pipeline.js";
import { ScanIndexer } from "../../helpers/ring-scan.js";

const SALT = new Uint8Array(32).fill(1) as Bytes32;

beforeAll(async () => {
  await initializePoseidon();
});

function depositClient(
  fixture: Fixture,
  proofDataSource: RingDepositClient["proofDataSource"] = "prover",
): RingDepositClient {
  return {
    ...fixture.client,
    proofDataSource,
    proofService: PROOF_SERVICE,
    getLatestBlockhash: async () => BLOCKHASH,
  };
}

describe("ring deposit", () => {
  it("proves a confidential deposit as a transfer that spends no note, the funder signing its leg", async () => {
    const fixture = await ringFixture();
    const recipient = ShieldedKeypair.generate();
    fixture.enroll(recipient.shieldedAddress());
    const funder = randomAddress();
    const client = depositClient(fixture);
    const prepared = await PreparedRingDeposit.prepare({
      client,
      ringProgramId: fixture.ringProgramId,
      feePayer: fixture.authority,
      depositor: funder,
      recipient: { kind: "confidential", address: recipient.shieldedAddress() },
      amount: 25n,
    });
    const proven = await prepared.prove(client);

    expect(proven.instruction.data?.[0]).toBe(RingInstructionTag.transfer);
    const [call] = fixture.sppCalls;
    expect(call?.proofInputs.inputUtxos.every((spent) => spent.isDummy())).toBe(true);
    expect(call?.proofInputs.externalData.interfaceTransfers).toEqual([
      { kind: "sol", isDeposit: true, amount: 25n, userSolAccount: funder },
    ]);
    expect(proven.instruction.accounts?.filter((meta) => meta.address === funder)).toEqual([
      expect.objectContaining({ role: AccountRole.WRITABLE_SIGNER }),
    ]);
    expect(fixture.ringRequests.map((request) => request.kind)).toEqual(["transfer"]);
    expect(proven.utxo.amount).toBe(25n);
    expect(proven.utxo.owner.equals(recipient.signingPublicKey())).toBe(true);
    await expect(prepared.prove(client)).rejects.toMatchObject({
      causeCode: "RING_DEPOSIT_FRAME_REUSED",
    });
  });

  it("refuses a client that assembles proof data itself before any proof", async () => {
    const fixture = await ringFixture();
    const recipient = ShieldedKeypair.generate();
    fixture.enroll(recipient.shieldedAddress());
    await expect(
      PreparedRingDeposit.prepare({
        client: depositClient(fixture, "client"),
        ringProgramId: fixture.ringProgramId,
        feePayer: fixture.authority,
        recipient: { kind: "confidential", address: recipient.shieldedAddress() },
        amount: 25n,
      }),
    ).rejects.toMatchObject({
      code: "RING_BUILD_DEPOSIT",
      causeCode: "RING_DEPOSIT_PROOF_DATA_SOURCE",
    });
    expect(fixture.sppCalls).toEqual([]);
    expect(fixture.ringRequests).toEqual([]);
  });

  it("pays an anonymous recipient a plan slot under its request tag, never its bootstrap tag", async () => {
    const fixture = await ringFixture({ mode: "anonymous" });
    const recipient = ShieldedKeypair.generate();
    fixture.enroll(recipient.shieldedAddress());
    const to = new RingAddress({
      recipient: recipient.shieldedAddress(),
      ringProgramId: fixture.ringProgramId,
      listSalt: SALT,
    });
    const request = new RingPaymentRequest({
      address: to,
      tag: new Uint8Array(32).fill(0x5a) as Bytes32,
    });
    const tags = await freshWalletTags(ShieldedKeypair.generate());
    const client = depositClient(fixture);
    const deposit = {
      client,
      ringProgramId: fixture.ringProgramId,
      feePayer: fixture.authority,
      amount: 40n,
    };
    // A deposit publishes its funder, a bootstrap tag beside it would name the recipient.
    const bootstrap = new RingPaymentRequest({
      address: to,
      tag: recipient.viewingPublicKey().u(),
    });
    // A deposit to the wallet's own bare address takes that wallet's sender tag alone, another wallet's tags leave it untagged.
    const own = { kind: "anonymous", keys: walletKeys(recipient), listSalt: SALT } as const;
    for (const selected of [{}, { tags }]) {
      const proved = fixture.sppCalls.length;
      for (const paid of [to, bootstrap])
        await expect(
          PreparedRingDeposit.prepare({
            ...deposit,
            recipient: { kind: "anonymousPayment", to: paid },
            ...selected,
          }),
        ).rejects.toMatchObject({ causeCode: "RING_DEPOSIT_BOOTSTRAP_TAG" });
      await expect(
        PreparedRingDeposit.prepare({ ...deposit, recipient: own, ...selected }),
      ).rejects.toMatchObject({ causeCode: "RING_DEPOSIT_BOOTSTRAP_TAG" });
      expect(fixture.sppCalls).toHaveLength(proved);
      const prepared = await PreparedRingDeposit.prepare({
        ...deposit,
        recipient: { kind: "anonymousPayment", to: request },
        ...selected,
      });
      await prepared.prove(client);
      const published = indexedFrame(fixture).outputSlots.map((slot) => slot.viewTag);
      // The fresh sender holds another key than the tags derive from, its change takes random bytes.
      expect(published[1]).toEqual(slotTag(request.tag as Bytes32));
      expect(published).not.toContainEqual(tags.sender);
    }

    const indexer = new ScanIndexer();
    indexer.publish(indexedFrame(fixture));
    const keys = walletKeys(recipient);
    const scan = await AnonymousSync.create(keys, new AssetRegistry()).fetch(
      indexer.client(),
      ViewingHistory.create(recipient.shieldedAddress().viewingPublicKey, 0n).withRing(
        fixture.ringProgramId,
      ),
    );
    expect(scan.utxos.map((note) => [note.utxo.amount, note.utxo.asset])).toEqual([
      [40n, SOL_MINT],
    ]);
  });

  it("refuses a recipient of the other ring mode and a ring address of another ring before proving", async () => {
    const confidential = await ringFixture();
    const anonymous = await ringFixture({ mode: "anonymous" });
    const owner = ShieldedKeypair.generate();
    const keys = walletKeys(owner);
    const attempt = (
      fixture: Fixture,
      recipient: Parameters<typeof PreparedRingDeposit.prepare>[0]["recipient"],
    ) =>
      PreparedRingDeposit.prepare({
        client: depositClient(fixture),
        ringProgramId: fixture.ringProgramId,
        feePayer: fixture.authority,
        recipient,
        amount: 1n,
      });
    await expect(
      attempt(anonymous, { kind: "confidential", address: owner.shieldedAddress() }),
    ).rejects.toMatchObject({ causeCode: "RING_ANONYMOUS_REFUSED" });
    await expect(
      attempt(confidential, { kind: "anonymous", keys, listSalt: SALT }),
    ).rejects.toMatchObject({ causeCode: "RING_NOT_ANONYMOUS" });
    const foreign = new RingAddress({
      recipient: owner.shieldedAddress(),
      ringProgramId: randomAddress(),
      listSalt: SALT,
    });
    await expect(
      attempt(anonymous, { kind: "anonymousPayment", to: foreign }),
    ).rejects.toMatchObject({ causeCode: "RING_FOREIGN_RING" });
    expect([...confidential.sppCalls, ...anonymous.sppCalls]).toEqual([]);
  });

  it("refuses a public signer equal to an anonymous recipient's owner key, a confidential ring takes it", async () => {
    const anonymous = await ringFixture({ mode: "anonymous" });
    const confidential = await ringFixture();
    // A browser identity's owner key is its wallet key.
    const owner = ShieldedKeypair.fromKeypair(SigningKey.generate());
    const wallet = owner.shieldedAddress().solanaAddress();
    const tags = await freshWalletTags(owner);
    const deposit = (
      fixture: Fixture,
      linked: Partial<Pick<RingDepositParams, "feePayer" | "depositor" | "cosigner">>,
    ) => {
      fixture.enroll(owner.shieldedAddress());
      return PreparedRingDeposit.prepare({
        client: depositClient(fixture),
        ringProgramId: fixture.ringProgramId,
        feePayer: fixture.authority,
        recipient:
          fixture === anonymous
            ? { kind: "anonymous", keys: walletKeys(owner), listSalt: SALT }
            : { kind: "confidential", address: owner.shieldedAddress() },
        tags,
        amount: 3n,
        ...linked,
      });
    };
    for (const linked of [{ depositor: wallet }, { feePayer: wallet }, { cosigner: wallet }]) {
      await expect(deposit(anonymous, linked)).rejects.toMatchObject({
        causeCode: "RING_ANONYMOUS_SIGNER_LINKS_OWNER",
      });
      await expect(deposit(confidential, linked)).resolves.toBeInstanceOf(PreparedRingDeposit);
    }
    expect(anonymous.sppCalls).toEqual([]);
  });

  it("relays a deposit on the deposit rail under the funder's request text and refuses a funding fee payer", async () => {
    const fixture = await ringFixture({ mode: "anonymous" });
    const owner = ShieldedKeypair.generate();
    fixture.enroll(owner.shieldedAddress());
    const signing = SigningKey.generate();
    const funder = getBase58Decoder().decode(signing.publicKey().ed25519()) as Address;
    const depositorSigner = {
      address: funder,
      signMessages: async (messages: readonly { content: Uint8Array }[]) =>
        messages.map((message) => ({ [funder]: signing.sign(message.content) })),
    } as never;
    const params = {
      client: depositClient(fixture),
      ringProgramId: fixture.ringProgramId,
      depositor: funder,
      depositorSigner,
      recipient: { kind: "anonymous", keys: walletKeys(owner), listSalt: SALT } as const,
      tags: await freshWalletTags(owner),
      amount: 9n,
      ringRpc: {
        cluster: new Uint8Array(32).fill(1) as Bytes32,
        service: getBase58Decoder().decode(new Uint8Array(32).fill(2)) as Address,
      },
    };
    const { relay } = await PreparedRingDeposit.prepareRelay({
      ...params,
      quote: { feePayer: fixture.authority, validUntilSlot: 100n },
    });
    const { intent } = relay.request;
    if (intent.operation !== "deposit") throw new Error("a deposit relay");
    const payload = new Uint8Array(getBase64Encoder().encode(intent.proofInputs));
    expect(payload[0]).toBe(3);
    const decoded = await decodeRelayProofInputs(payload, new AssetRegistry());
    expect(decoded.rail.kind).toBe("deposit");
    expect(decoded.inputs.every((spent) => spent.isDummy())).toBe(true);
    // A deposit to the wallet's own key takes its sender tag, the wallet's sender stream finds it.
    expect(
      decoded.externalData.outputs.map((output) =>
        output.ownerTag.kind === "inline" ? output.ownerTag.value : undefined,
      ),
    ).toContainEqual(params.tags.sender);
    const text = relayRequestAttestation({
      target: { ...params.ringRpc, ringProgramId: fixture.ringProgramId },
      signer: funder,
      request: {
        kind: "deposit",
        digest: depositRequestDigest(payload),
        legs: [{ isDeposit: true, mint: SOL_MINT, amount: 9n }],
      },
    });
    expect(new TextDecoder().decode(text)).toContain(`\ndeposit: ${SOL_MINT} 9\n`);
    const signature = new Uint8Array(getBase58Encoder().encode(intent.depositorSignature));
    expect(signing.verify(text, signature as Bytes64)).toBe(true);
    expect(signing.verify(depositRequestDigest(payload), signature as Bytes64)).toBe(false);
    expect(fixture.sppCalls).toEqual([]);
    const sent = await relay.send({
      prepareRelay: async () => ({ step: "signMessage", relayId: relay.relayId(), message: "" }),
    } as never);
    if (!(sent instanceof PendingRelaySignature)) throw new Error("a pending signature");
    expect(sent.funderTerms(funder)?.slots).toEqual(
      decoded.externalData.outputs.map(({ utxoHash, ownerTag }) => ({
        hash: utxoHash,
        ...(ownerTag.kind === "inline" ? { tag: ownerTag.value } : {}),
      })),
    );

    await expect(
      PreparedRingDeposit.prepareRelay({
        ...params,
        quote: { feePayer: funder, validUntilSlot: 100n },
      }),
    ).rejects.toMatchObject({ causeCode: "RING_RELAY_FEE_PAYER_FUNDS_DEPOSIT" });
  });
});
