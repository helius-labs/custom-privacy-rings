import {
  AccountRole,
  getBase58Decoder,
  getBase64Encoder,
  getProgramDerivedAddress,
  isOffCurveAddress,
  type Address,
} from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { initializePoseidon } from "@heliuslabs/spr";

import { TransactWithdrawal } from "@heliuslabs/spr/interface";
import { treeAddress } from "@heliuslabs/spr/internal";
import {
  NullifierKey,
  ShieldedKeypair,
  SigningKey,
  ViewingKey,
  type Bytes31,
} from "@heliuslabs/spr/keypair";
import { ShieldedPda } from "@heliuslabs/spr/program";
import {
  AssetRegistry,
  ConfidentialTransfer,
  SOL_MINT,
  WithdrawalTarget,
  type ProofInputUtxo,
  type ProofOutputUtxo,
} from "@heliuslabs/spr/transaction";

import { BLOCKHASH } from "../../../../../zolana/sdk-libs/ts/test/helpers/clients.js";
import { RingAddress } from "../../../src/crypto/keys/ring-address.js";
import { signingKeySpendSigner } from "../../../src/operations/payments/anonymous.js";
import { CustomRingTransfer } from "../../../src/operations/payments/transfer-rails.js";
import {
  anonymousRingTransfers,
  ownerRingTransfers,
} from "../../../src/operations/payments/transfer.js";
import { ringTransactInstruction } from "../../../src/operations/workflow/pipeline.js";
import {
  transactionHash,
  type ProgramCommitment,
} from "../../../src/operations/workflow/program-commitment.js";
import { RING_COSIGN_TRANSFERS } from "../../../src/protocol/codec/ring-codecs.js";
import { listTagOf } from "../../../src/crypto/keys/list-salt.js";
import { buildRuleTable, ListId } from "../../../src/protocol/membership/policy.js";
import type { Bytes32 } from "@heliuslabs/spr/interface";
import { INSTRUCTIONS_SYSVAR, ringCoSignerAddress } from "../../../src/protocol/pda/index.js";
import {
  TransferApproval,
  approveTransferInstruction,
  approvedTransferCall,
} from "../../../src/protocol/transaction/transfer-approval.js";
import {
  filled,
  hex,
  noteInput,
  pdaKeys,
  randomAddress,
  ringError,
  ringFixture,
  small,
  walletKeys,
  type Fixture,
} from "../../helpers/ring-pipeline.js";

const assets = new AssetRegistry();

/** A fresh builder over the same note, a rail's payment writes into its transfer. */
function ownerRail(fixture: Fixture, owner: ShieldedKeypair, note: ProofInputUtxo) {
  fixture.enroll(owner.shieldedAddress());
  const transfer = new ConfidentialTransfer(owner.shieldedAddress(), [note], fixture.authority);
  const rail = CustomRingTransfer.new({
    ringProgramId: fixture.ringProgramId,
    keys: walletKeys(owner),
    transfer,
    assets,
    ...(fixture.coSigner === undefined ? {} : { coSigners: [fixture.coSigner] }),
  });
  return { transfer, rail };
}

const blindings = (outputs: readonly ProofOutputUtxo[]) =>
  outputs.map((output) => hex(output.blinding));

describe("program transfer", () => {
  beforeAll(initializePoseidon);

  it.each(["confidential", "anonymous"] as const)(
    "stages a committed transfer to its plan on the public rail of a %s ring",
    async (mode) => {
      const fixture = await ringFixture({ mode });
      const owner = ShieldedKeypair.generate();
      const paid = ShieldedKeypair.generate();
      fixture.enroll(paid.shieldedAddress());
      const to = new RingAddress({
        recipient: paid.shieldedAddress(),
        ringProgramId: fixture.ringProgramId,
        listSalt: filled(1),
      });
      const note = noteInput(owner, 10n, fixture.ringProgramId);
      const transfer = () =>
        ownerRail(fixture, owner, note)
          .rail.spendPublic(owner.shieldedAddress().solanaAddress())
          .withPayment({ to, asset: SOL_MINT, amount: 4n, reply: null });

      const plan = await transfer().planForProgram(fixture.client);
      expect(plan.outputs.slice(0, 2).map((output) => output.amount)).toEqual([6n, 4n]);
      expect(plan.transactionHash).toEqual(plan.privateTxHash);

      const proven = await transfer().withProgram(plan.commitment()).prove(fixture.client);
      const staged = fixture.sppCalls.at(-1)?.proofInputs;
      expect(staged?.externalData.salt).toEqual(plan.salt);
      expect(proven.data.salt).toEqual(plan.salt);
      expect(staged?.blindingSeed).toEqual(plan.blindingSeed);
      expect(staged?.checkShape()).toEqual(plan.shape);
      expect(staged?.firstNullifier()).toEqual(plan.firstNullifier);
      expect(blindings(staged?.outputs ?? [])).toEqual(blindings(plan.outputs));
      expect(staged?.outputs.map((output) => output.amount)).toEqual(
        plan.outputs.map((output) => output.amount),
      );
      expect(proven.data.privateTxHash).toEqual(plan.privateTxHash);

      const proofs = [fixture.sppCalls.length, fixture.ringRequests.length];
      await expect(
        transfer()
          .withProgram({ ...plan.commitment(), privateTxHash: small(1) })
          .prove(fixture.client),
      ).rejects.toThrow(ringError("RING_PROGRAM_COMMITMENT_MISMATCH"));
      expect([fixture.sppCalls.length, fixture.ringRequests.length]).toEqual(proofs);
    },
  );

  it("refuses a commitment where an owner signs inside a proof", async () => {
    const signing = SigningKey.generate();
    const owner = ShieldedKeypair.fromKeypair(signing);
    const signer = signingKeySpendSigner(signing);
    const refused = ringError("RING_PROGRAM_RAIL_UNSUPPORTED");

    const confidential = await ringFixture();
    const note = noteInput(owner, 10n, confidential.ringProgramId);
    const rail = () => ownerRail(confidential, owner, note).rail;
    const plan = await rail().planForProgram(confidential.client);
    await expect(
      rail().withProgram(plan.commitment()).prove(confidential.client),
    ).resolves.toBeDefined();
    const inProof = () => rail().withInProofOwner(signer);
    await expect(inProof().planForProgram(confidential.client)).rejects.toThrow(refused);
    await expect(
      inProof().withProgram(plan.commitment()).prove(confidential.client),
    ).rejects.toThrow(refused);

    const anonymous = await ringFixture({ mode: "anonymous" });
    const hidden = ownerRail(anonymous, owner, noteInput(owner, 10n, anonymous.ringProgramId))
      .rail.withProgram(plan.commitment())
      .withAnonymousSigner(signer);
    await expect(hidden.prove(anonymous.client)).rejects.toThrow(refused);
    expect(anonymous.sppCalls).toHaveLength(0);
  });

  it("takes each co-signer by an approval of the built call", async () => {
    const approver = randomAddress();
    const fixture = await ringFixture({
      coSigner: { signer: approver, scope: RING_COSIGN_TRANSFERS },
    });
    const owner = ShieldedKeypair.generate();
    const wallet = owner.shieldedAddress().solanaAddress();
    const recipient = randomAddress();
    const note = noteInput(owner, 10_000n, fixture.ringProgramId);
    const withdrawing = () => {
      const { transfer, rail } = ownerRail(fixture, owner, note);
      transfer.withdraw(SOL_MINT, 200n, WithdrawalTarget.sol({ recipient }));
      return rail.withWithdrawalAccounts(TransactWithdrawal.sol({ recipient }));
    };
    const plan = await withdrawing().planForProgram(fixture.client);
    expect(plan.transactionHash).not.toEqual(plan.privateTxHash);
    await expect(
      withdrawing()
        .withProgram({ ...plan.commitment(), transactionHash: small(1) })
        .prove(fixture.client),
    ).rejects.toThrow(ringError("RING_PROGRAM_TRANSACTION_HASH_MISMATCH"));
    expect(fixture.sppCalls).toHaveLength(0);

    const record = await ringCoSignerAddress(fixture.ringProgramId, approver);
    const roleOf = (accounts: readonly { address: Address; role: AccountRole }[], of: Address) =>
      accounts.find((account) => account.address === of)?.role;
    const afterRecord = (accounts: readonly { address: Address; role: AccountRole }[]) => {
      const pair = accounts[accounts.findIndex((account) => account.address === record) + 1];
      return { address: pair?.address, role: pair?.role };
    };

    const committed = await withdrawing().withProgram(plan.commitment()).prove(fixture.client);
    const call = await committed.instruction();
    const accounts = call.accounts ?? [];
    expect(afterRecord(accounts)).toEqual({
      address: INSTRUCTIONS_SYSVAR,
      role: AccountRole.READONLY,
    });
    expect(roleOf(accounts, approver)).toBeUndefined();
    expect(roleOf(accounts, wallet)).toBe(AccountRole.READONLY_SIGNER);
    expect(roleOf(accounts, fixture.authority)).toBe(AccountRole.WRITABLE_SIGNER);
    const approval = { position: 3, digest: TransferApproval.digest(approvedTransferCall(call)) };
    expect(await committed.approveTransfer(3)).toEqual([
      await approveTransferInstruction({
        ringProgramId: fixture.ringProgramId,
        coSigner: approver,
        approval,
      }),
    ]);
    const approved = await committed.approvedTransfer();
    expect(new Uint8Array(getBase64Encoder().encode(approved.data))).toEqual(call.data);
    expect(approved.accounts).toEqual(accounts.map((account) => account.address));

    const direct = await withdrawing().prove(fixture.client);
    const signed = (await direct.instruction()).accounts ?? [];
    expect(afterRecord(signed)).toEqual({ address: approver, role: AccountRole.READONLY_SIGNER });
    expect(roleOf(signed, wallet)).toBe(AccountRole.READONLY_SIGNER);
    const outside = ringError("RING_APPROVAL_OUTSIDE_PROGRAM");
    await expect(direct.approveTransfer(3)).rejects.toThrow(outside);
    await expect(direct.approvedTransfer()).rejects.toThrow(outside);
  });

  it("spends a PDA's note and keeps the signature of a wallet co-owner", async () => {
    const fixture = await ringFixture();
    const [pda] = await getProgramDerivedAddress({
      programAddress: randomAddress(),
      seeds: ["owner"],
    });
    expect(isOffCurveAddress(pda)).toBe(true);
    const holder = ShieldedPda.withViewingKey(
      pda,
      NullifierKey.fromSecret(new Uint8Array(31).fill(3) as Bytes31),
      ViewingKey.generate(),
    );
    const paid = ShieldedKeypair.generate();
    fixture.enroll(holder.shieldedAddress());
    fixture.enroll(paid.shieldedAddress());
    const to = new RingAddress({
      recipient: paid.shieldedAddress(),
      ringProgramId: fixture.ringProgramId,
      listSalt: filled(1),
    });
    const note = noteInput(holder, 10n, fixture.ringProgramId);
    const transfer = () =>
      CustomRingTransfer.new({
        ringProgramId: fixture.ringProgramId,
        keys: pdaKeys(holder),
        transfer: new ConfidentialTransfer(holder.shieldedAddress(), [note], fixture.authority),
        assets,
      })
        .spendPublic(pda)
        .withPayment({ to, asset: SOL_MINT, amount: 4n, reply: null });

    const plan = await transfer().planForProgram(fixture.client);
    const proven = await transfer().withProgram(plan.commitment()).prove(fixture.client);
    expect(proven.accounts.ownerSigners).toEqual([pda]);
    const roles = async (ownerSigners: readonly Address[]) => {
      const call = await ringTransactInstruction(
        proven.ringProgramId,
        { ...proven.accounts, ownerSigners },
        proven.data,
      );
      return ownerSigners.map(
        (owner) => call.accounts?.find((account) => account.address === owner)?.role,
      );
    };
    expect(await roles([pda])).toEqual([AccountRole.READONLY]);
    const wallet = ShieldedKeypair.generate().shieldedAddress().solanaAddress();
    expect(await roles([pda, wallet])).toEqual([AccountRole.READONLY, AccountRole.READONLY_SIGNER]);
  });

  // Rust `transaction_hash` over the same legs.
  it("chains the public transfers to the hashes Rust produces", () => {
    const address = (seed: number) => getBase58Decoder().decode(filled(seed)) as Address;
    const legs = [
      { mint: address(4), isDeposit: false, amount: 200n, tokenAccount: address(6) },
      { mint: address(7), isDeposit: true, amount: 1_000_000_007n, tokenAccount: address(8) },
    ].map((leg) => ({ kind: "spl" as const, splInterfaceBump: 0, ...leg }));
    const privateTxHash = small(5);
    expect(transactionHash(privateTxHash, [])).toEqual(privateTxHash);
    expect(hex(transactionHash(privateTxHash, legs.slice(0, 1)))).toBe(
      "1946fea3144a0e0c44ac91af3195b7810a9d4d42c65266ad02bbf205cb73da12",
    );
    expect(hex(transactionHash(privateTxHash, legs))).toBe(
      "1fc63988477be7e64c70cadc6e4c05c14b72aefffafaec6d194dd3cac0c59d54",
    );
  });
});

describe("builder lines", () => {
  beforeAll(initializePoseidon);

  const forms = (["confidential", "anonymous"] as const).flatMap((mode) =>
    ([false, true] as const).map((program) => ({ mode, program })),
  );

  it.each(forms)(
    "refuses a value another step has passed on a $mode ring, program $program",
    async ({ mode, program }) => {
      const fixture = await ringFixture({ mode });
      const owner = ShieldedKeypair.generate();
      const [alice, bob] = [ShieldedKeypair.generate(), ShieldedKeypair.generate()];
      const pay = (paid: ShieldedKeypair) => {
        fixture.enroll(paid.shieldedAddress());
        const to = new RingAddress({
          recipient: paid.shieldedAddress(),
          ringProgramId: fixture.ringProgramId,
          listSalt: filled(1),
        });
        return { to, asset: SOL_MINT, amount: 2n, reply: null };
      };
      const superseded = ringError("RING_TRANSFER_SUPERSEDED");
      const { rail } = ownerRail(fixture, owner, noteInput(owner, 10n, fixture.ringProgramId));
      const base = rail.spendPublic(owner.shieldedAddress().solanaAddress());
      const toAlice = base.withPayment(pay(alice));
      // The base and the rail behind it hold no recipient and read nothing.
      expect(() => base.withPayment(pay(bob))).toThrow(superseded);
      const commitment = {
        salt: new Uint8Array(16) as ProgramCommitment["salt"],
        privateTxHash: small(1),
        transactionHash: small(1),
      };
      expect(() => base.withProgram(commitment)).toThrow(superseded);
      await expect(base.planForProgram(fixture.client)).rejects.toThrow(superseded);
      await expect(base.prove(fixture.client)).rejects.toThrow(superseded);
      await expect(rail.prove(fixture.client)).rejects.toThrow(superseded);
      expect(fixture.sppCalls).toHaveLength(0);

      const paid = (outputs: readonly ProofOutputUtxo[]) =>
        outputs
          .filter((output) => output.ownerAddress !== undefined)
          .map((output) => hex(output.ownerAddress?.toBytes() ?? new Uint8Array()))
          .filter((address) => address !== hex(owner.shieldedAddress().toBytes()));
      const recipients = [alice, bob].map((keypair) => hex(keypair.shieldedAddress().toBytes()));
      const live = toAlice.withPayment(pay(bob));
      await expect(toAlice.prove(fixture.client)).rejects.toThrow(superseded);
      const plan = await live.planForProgram(fixture.client);
      expect(paid(plan.outputs)).toEqual(recipients);
      const proven = await (program ? live.withProgram(plan.commitment()) : live).prove(
        fixture.client,
      );
      expect(paid(proven.outputs)).toEqual(recipients);

      // A second line over the same transfer supersedes the first.
      const again = ownerRail(fixture, owner, noteInput(owner, 10n, fixture.ringProgramId));
      const first = CustomRingTransfer.new({
        ringProgramId: fixture.ringProgramId,
        keys: walletKeys(owner),
        transfer: again.transfer,
        assets,
      });
      expect(() => again.rail.withExactApproval()).toThrow(superseded);
      expect(first.withExactApproval()).toBeDefined();
    },
  );
});

describe("builder steps", () => {
  beforeAll(initializePoseidon);

  const address = (fixture: Fixture, paid: ShieldedKeypair, listSalt = filled(1)) => {
    fixture.enroll(paid.shieldedAddress());
    return new RingAddress({
      recipient: paid.shieldedAddress(),
      ringProgramId: fixture.ringProgramId,
      listSalt,
    });
  };
  const application = { hash: filled(0x11), bytes: Uint8Array.of(1, 2, 3) };

  it("refuses a second line over a transfer that holds a ring payment", async () => {
    const fixture = await ringFixture();
    const owner = ShieldedKeypair.generate();
    const { transfer, rail } = ownerRail(
      fixture,
      owner,
      noteInput(owner, 10n, fixture.ringProgramId),
    );
    const paying = rail.spendPublic(owner.shieldedAddress().solanaAddress()).withPayment({
      to: address(fixture, ShieldedKeypair.generate()),
      asset: SOL_MINT,
      amount: 4n,
      reply: null,
      data: { application },
    });
    expect(() =>
      CustomRingTransfer.new({
        ringProgramId: fixture.ringProgramId,
        keys: walletKeys(owner),
        transfer,
        assets,
      }),
    ).toThrow(ringError("RING_TRANSFER_SUPERSEDED"));
    const plan = await paying.planForProgram(fixture.client);
    expect(plan.outputs.map((output) => hex(output.dataHash ?? new Uint8Array(32)))).toContain(
      hex(application.hash),
    );
  });

  it.each(["public", "hidden"] as const)(
    "leaves a %s builder as it was when a payment is refused",
    async (rail) => {
      const fixture = await ringFixture({ mode: "anonymous" });
      const signing = SigningKey.generate();
      const owner = ShieldedKeypair.fromKeypair(signing);
      const [paid, held] = [ShieldedKeypair.generate(), ShieldedKeypair.generate()];
      const base = ownerRail(fixture, owner, noteInput(owner, 10n, fixture.ringProgramId)).rail;
      const builder =
        rail === "public"
          ? base.spendPublic(owner.shieldedAddress().solanaAddress())
          : base.withAnonymousSigner(signingKeySpendSigner(signing));
      const to = address(fixture, paid);
      const payment = { to, asset: SOL_MINT, amount: 4n, reply: null };
      // A negative amount, a zero data hash and a salt the builder holds otherwise each write nothing.
      expect(() => builder.withPayment({ ...payment, amount: -1n })).toThrow();
      expect(() =>
        builder.withPayment({
          ...payment,
          data: { application: { ...application, hash: small(0) } },
        }),
      ).toThrow(ringError("RING_NOTE_DATA_UNBOUND"));
      const salted = builder.withPayment({ ...payment, to: address(fixture, held, filled(2)) });
      expect(() =>
        salted.withPayment({ ...payment, to: address(fixture, held, filled(3)) }),
      ).toThrow(ringError("RING_ADDRESS_MISMATCH"));

      const proven = await salted.withPayment(payment).prove(fixture.client);
      const owners = proven.outputs
        .filter((output) => output.ownerAddress !== undefined && output.amount > 0n)
        .map((output) => hex(output.ownerAddress?.toBytes() ?? new Uint8Array()));
      expect(owners).toEqual(
        [owner, held, paid].map((keypair) => hex(keypair.shieldedAddress().toBytes())),
      );
      expect(proven.outputs.map((output) => output.amount).filter((amount) => amount > 0n)).toEqual(
        [2n, 4n, 4n],
      );
    },
  );
});

describe("ring transfer adapters", () => {
  beforeAll(initializePoseidon);

  /** Another output tree than a fresh transfer names, the adapter's write then shows. */
  const client = (fixture: Fixture) => ({
    ...fixture.client,
    tree: treeAddress(fixture.client.treeId + 1),
    treeId: fixture.client.treeId + 1,
    getLatestBlockhash: async () => BLOCKHASH,
  });
  const recipients = (transfer: ConfidentialTransfer) => {
    const prepared = transfer.prepare();
    return {
      outputTreeId: prepared.outputTreeId,
      paid: prepared.outputs
        .slice(prepared.senderOutputCount)
        .map((output) => [hex(output.ownerAddress?.toBytes() ?? new Uint8Array()), output.amount]),
    };
  };

  it.each(["confidential", "anonymous"] as const)(
    "refuses a transfer a builder line holds and leaves it unchanged on a %s ring",
    async (mode) => {
      const fixture = await ringFixture({ mode });
      const signing = SigningKey.generate();
      const owner = ShieldedKeypair.fromKeypair(signing);
      const [alice, bob] = [ShieldedKeypair.generate(), ShieldedKeypair.generate()];
      for (const paid of [alice, bob]) fixture.enroll(paid.shieldedAddress());
      const { transfer, rail } = ownerRail(
        fixture,
        owner,
        noteInput(owner, 10n, fixture.ringProgramId),
      );
      const held = rail.spendPublic(owner.shieldedAddress().solanaAddress()).withPayment({
        to: new RingAddress({
          recipient: alice.shieldedAddress(),
          ringProgramId: fixture.ringProgramId,
          listSalt: filled(1),
        }),
        asset: SOL_MINT,
        amount: 4n,
        reply: null,
      });
      const before = recipients(transfer);
      const input = { client: client(fixture), ringProgramId: fixture.ringProgramId, assets };
      const payee = new RingAddress({
        recipient: bob.shieldedAddress(),
        ringProgramId: fixture.ringProgramId,
        listSalt: filled(1),
      });
      const adapter =
        mode === "confidential"
          ? ownerRingTransfers(input)
          : anonymousRingTransfers({ ...input, payees: [payee] });
      await expect(
        adapter.proveRingTransfer({
          transfer,
          payment: { recipient: bob.shieldedAddress(), asset: SOL_MINT, amount: 2n },
          keys: walletKeys(owner),
          feePayer: fixture.authority,
          owner: signing.publicKey(),
          authorize: async () => undefined,
        }),
      ).rejects.toThrow(ringError("RING_TRANSFER_SUPERSEDED"));
      expect(recipients(transfer)).toEqual(before);
      const plan = await held.planForProgram(fixture.client);
      expect(plan.outputs.map((output) => output.amount).filter((amount) => amount > 0n)).toEqual([
        6n,
        4n,
      ]);
    },
  );

  it("never pays twice when a failed proof is tried again with the same request", async () => {
    const fixture = await ringFixture();
    const owner = ShieldedKeypair.generate();
    const bob = ShieldedKeypair.generate();
    fixture.enroll(owner.shieldedAddress());
    fixture.enroll(bob.shieldedAddress());
    const transfer = new ConfidentialTransfer(
      owner.shieldedAddress(),
      [noteInput(owner, 10n, fixture.ringProgramId)],
      fixture.authority,
    );
    const request = {
      transfer,
      payment: { recipient: bob.shieldedAddress(), asset: SOL_MINT, amount: 2n },
      keys: walletKeys(owner),
      feePayer: fixture.authority,
      authorize: async () => undefined,
    };
    const ring = { ringProgramId: fixture.ringProgramId, assets };
    const down = {
      ...client(fixture),
      proveRing: async () => {
        throw new Error("prover down");
      },
    };
    await expect(
      ownerRingTransfers({ ...ring, client: down }).proveRingTransfer(request),
    ).rejects.toThrow("prover down");
    await expect(
      ownerRingTransfers({ ...ring, client: client(fixture) }).proveRingTransfer(request),
    ).rejects.toThrow(ringError("RING_TRANSFER_SUPERSEDED"));
    expect(recipients(transfer).paid).toEqual([[hex(bob.shieldedAddress().toBytes()), 2n]]);
  });
});

describe("payment address list salts", () => {
  beforeAll(initializePoseidon);

  const forms = (["confidential", "anonymous"] as const).flatMap((mode) =>
    ([false, true] as const).map((program) => ({ mode, program })),
  );

  it.each(forms)(
    "reads the recipient's salt from the address on a $mode ring, program $program",
    async ({ mode, program }) => {
      const fixture = await ringFixture({
        mode,
        table: buildRuleTable({
          rules: [
            {
              subject: "outputOwner",
              source: { kind: "lists", present: [], absent: [ListId.privateBlock] },
              guard: { kind: "always" },
            },
          ],
        }),
      });
      const [owner, paid] = [ShieldedKeypair.generate(), ShieldedKeypair.generate()];
      const [ownSalt, paidSalt] = [filled(3), filled(5)];
      fixture.enroll(owner.shieldedAddress(), listTagOf(ownSalt));
      fixture.enroll(paid.shieldedAddress(), listTagOf(paidSalt));
      const member = (keypair: ShieldedKeypair) =>
        keypair.shieldedAddress().signingPublicKey.ownerProofInputHash();
      const note = noteInput(owner, 10n, fixture.ringProgramId);
      const transfer = (listSalts: readonly { owner: Bytes32; salt: Bytes32 }[]) =>
        CustomRingTransfer.new({
          ringProgramId: fixture.ringProgramId,
          keys: walletKeys(owner),
          transfer: new ConfidentialTransfer(owner.shieldedAddress(), [note], fixture.authority),
          assets,
          listSalts: [{ owner: member(owner), salt: ownSalt }, ...listSalts],
        })
          .spendPublic(owner.shieldedAddress().solanaAddress())
          .withPayment({
            to: new RingAddress({
              recipient: paid.shieldedAddress(),
              ringProgramId: fixture.ringProgramId,
              listSalt: paidSalt,
            }),
            asset: SOL_MINT,
            amount: 4n,
            reply: null,
          });
      const plan = await transfer([]).planForProgram(fixture.client);
      const built = program ? transfer([]).withProgram(plan.commitment()) : transfer([]);
      await built.prove(fixture.client);
      const request = fixture.ringRequests.at(-1);
      if (request?.kind !== "transfer") throw new Error("a transfer request");
      expect(request.ownerEntries.map((entry) => hex(entry.listSalt))).toContain(hex(paidSalt));

      expect(() => transfer([{ owner: member(paid), salt: filled(9) }])).toThrow(
        ringError("RING_ADDRESS_MISMATCH"),
      );
    },
  );
});
