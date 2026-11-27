/** A public funder signs for its own deposit legs and nothing else of its. */
import { AccountRole, type AccountMeta, type Address } from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { initializePoseidon } from "@heliuslabs/spr";
import { SOL_INTERFACE, type Bytes32 } from "@heliuslabs/spr/interface";
import { compileUnsignedTransaction, encodeBase58 } from "@heliuslabs/spr/internal";

import { ringComputeUnitLimit } from "../../../src/operations/management/admin.js";
import { SIZE_PROBE_LIFETIME } from "../../../src/operations/workflow/compile.js";
import { FunderApproval } from "../../../src/proof/cosign.js";
import { PendingRelaySignature } from "../../../src/rpc/relay.js";
import { anonymousPartyIdentities } from "../../../src/protocol/membership/parties.js";
import { decodeRingPolicy } from "../../../src/protocol/codec/ring-codecs.js";
import { ringAuthAddress, ringPolicyAddress } from "../../../src/protocol/pda/index.js";
import { RingInstructionTag } from "../../../src/protocol/transaction/instruction-tag.js";
import {
  checkRelayMessage,
  decodeRelayFunderTerms,
  encodeRelayFunderTerms,
  expectedFunder,
  type PublicLeg,
  type RelayFunderTerms,
} from "../../../src/rpc/relay-message.js";
import { RingTransactData } from "../../../src/protocol/transaction/transact.js";
import { ringPolicyData } from "../../helpers/ring-accounts.js";
import { field, ringFrame, ringInstructionData } from "../../helpers/ring-frame.js";

const key = (byte: number): Address => encodeBase58(new Uint8Array(32).fill(byte));
const RING = key(1);
const PAYER = key(2);
const FUNDER = key(3);
const MINT = key(4);
const READ = AccountRole.READONLY;
const WRITE = AccountRole.WRITABLE;
const SIGN_WRITE = AccountRole.WRITABLE_SIGNER;
const RELAY_ID = "11111111111111111111111111111111" as never;
const POLICY = decodeRingPolicy(ringPolicyData({ bump: 255, tracking: key(40) }));
const TRACKING: AccountMeta = { address: POLICY.tracking, role: AccountRole.READONLY };

beforeAll(initializePoseidon);

const FRAME = ringFrame({ rail: "confidential", inputs: 1, outputs: 2 });
const SLOTS: RelayFunderTerms["slots"] = FRAME.outputs.map(({ hash, ownerTag }) => ({
  hash,
  tag: ownerTag,
}));

const terms = (
  legs: RelayFunderTerms["legs"],
  slots: RelayFunderTerms["slots"] = SLOTS,
): RelayFunderTerms => ({
  ring: RING,
  feePayer: PAYER,
  signer: FUNDER,
  externalDataHash: RingTransactData.externalDataHash(
    {
      ...FRAME,
      interfaceTransfers: legs.map(({ amount, mint, direction }) =>
        mint === null
          ? { kind: direction === "deposit" ? "solDeposit" : "solWithdrawal", amount }
          : {
              kind: direction === "deposit" ? "splDeposit" : "splWithdrawal",
              amount,
              splInterfaceBump: 255,
            },
      ),
    },
    { auditorPubkey: POLICY.auditorPublicKey.toBytes() },
    legs.map(({ accounts, mint }) => ({
      asset: mint ?? accounts[0]!,
      user: accounts[mint === null ? 1 : 3]!,
    })),
  ),
  slots,
  legs,
  setup: [],
});

/** The ring account, its tracking registry, SPP's head, one tree, then the legs' settlement groups. */
async function message(
  input: Readonly<{
    transfers: RingTransactData["interfaceTransfers"];
    groups: readonly AccountMeta[];
    run?: readonly AccountMeta[];
    registry?: AccountMeta;
    outputs?: RingTransactData["outputs"];
    recipientKeys?: Uint8Array;
  }>,
): Promise<Uint8Array> {
  const frame: RingTransactData = {
    ...FRAME,
    interfaceTransfers: input.transfers,
    outputs: input.outputs ?? FRAME.outputs,
    recipientKeys: input.recipientKeys ?? FRAME.recipientKeys,
  };
  const spp = (at: number, role: AccountRole): AccountMeta => ({ address: key(at), role });
  const accounts: AccountMeta[] = [
    { address: await ringPolicyAddress(RING), role: READ },
    input.registry ?? TRACKING,
    { address: PAYER, role: SIGN_WRITE },
    spp(10, WRITE),
    spp(11, READ),
    spp(12, READ),
    { address: await ringAuthAddress(RING), role: READ },
    spp(13, WRITE),
    ...frame.nullifiers.map((_, at) => spp(20 + at, WRITE)),
    ...(input.run ?? []),
    ...input.groups,
  ];
  const { messageBytes } = compileUnsignedTransaction({
    feePayer: PAYER,
    lifetime: SIZE_PROBE_LIFETIME,
    instructions: [{ programAddress: RING, accounts, data: ringInstructionData(frame) }],
    computeUnitLimit: ringComputeUnitLimit(RingInstructionTag.transfer),
  });
  return new Uint8Array(messageBytes);
}

async function verdict(
  named: RelayFunderTerms,
  bytes: Uint8Array,
  signer: Address = FUNDER,
): Promise<string> {
  const expected = await expectedFunder({ terms: named, config: POLICY });
  try {
    checkRelayMessage(expected, bytes, signer);
    return "Accepted";
  } catch (error) {
    return String((error as { details?: { mismatch?: unknown } }).details?.mismatch);
  }
}

const solLeg = (amount: bigint) => ({ kind: "solDeposit" as const, amount });
const solGroup: readonly AccountMeta[] = [
  { address: key(30), role: WRITE },
  { address: FUNDER, role: SIGN_WRITE },
];
const sol = (amount: bigint): PublicLeg => ({
  direction: "deposit",
  mint: null,
  amount,
  accounts: solGroup.map((meta) => meta.address),
});

describe("a public funder's check", () => {
  it("passes the legs its terms name", async () => {
    const bytes = await message({ transfers: [solLeg(5n)], groups: solGroup });
    expect(await verdict(terms([sol(5n)]), bytes)).toBe("Accepted");
    const pending = await new FunderApproval({
      relayId: RELAY_ID,
      message: bytes,
      terms: terms([sol(5n)]),
      config: POLICY,
    }).pending();
    expect(pending.verify(FUNDER).signer).toBe(FUNDER);
  });

  it("refuses a different sealed recipient key for the same commitment", async () => {
    const bytes = await message({
      transfers: [solLeg(5n)],
      groups: solGroup,
      recipientKeys: FRAME.recipientKeys.map((byte) => byte ^ 1),
    });
    expect(await verdict(terms([sol(5n)]), bytes)).toBe("Data");
  });

  it("signs a sealed deposit frame through the funder's own terms and the returned message", async () => {
    const bytes = await message({ transfers: [solLeg(5n)], groups: solGroup });
    const named = terms([sol(5n)]);
    const sealed = new PendingRelaySignature(RELAY_ID, bytes, {
      kind: "sealed",
      terms: [],
      funders: [named],
      chargedOutflows: [],
    });
    expect(() => sealed.verify(FUNDER)).toThrow("RING_RELAY_MESSAGE_INVALID");
    const funder = sealed.funderTerms(FUNDER);
    if (funder === undefined) throw new Error("no funder terms");
    const pending = await new FunderApproval({
      relayId: RELAY_ID,
      message: sealed.message(),
      terms: funder,
      config: POLICY,
    }).pending();
    expect(pending.verify(FUNDER).signer).toBe(FUNDER);
  });

  it("refuses another amount, a missing leg and a leg it did not name", async () => {
    const one = await message({ transfers: [solLeg(5n)], groups: solGroup });
    expect(await verdict(terms([sol(6n)]), one)).toBe("Legs");
    expect(await verdict(terms([sol(5n), sol(7n)]), one)).toBe("Legs");
    const two = await message({
      transfers: [solLeg(5n), solLeg(7n)],
      groups: [...solGroup, { address: key(30), role: WRITE }, solGroup[1] as AccountMeta],
    });
    expect(await verdict(terms([sol(5n)]), two)).toBe("Legs");
    expect(await verdict(terms([sol(7n), sol(5n)]), two)).toBe("Legs");
    expect(await verdict(terms([sol(5n), sol(7n)]), two)).toBe("Accepted");
  });

  it("refuses its exact leg in a frame that publishes other outputs", async () => {
    const outputs = FRAME.outputs.map((output, at) =>
      at === 1 ? { ...output, hash: field(90) } : output,
    );
    const bytes = await message({ transfers: [solLeg(5n)], groups: solGroup, outputs });
    expect(await verdict(terms([sol(5n)]), bytes)).toBe("Recipients");
    const untagged = SLOTS.map((slot, at) => (at === 0 ? { hash: slot.hash } : slot));
    const plain = await message({ transfers: [solLeg(5n)], groups: solGroup });
    expect(await verdict(terms([sol(5n)], untagged), plain)).toBe("Recipients");
    expect(await verdict(terms([sol(5n)], SLOTS.slice(0, 1)), plain)).toBe("UnopenedOutput");
  });

  it("refuses its exact leg beside a withdrawal leg its terms do not name", async () => {
    const bytes = await message({
      transfers: [solLeg(5n), { kind: "solWithdrawal", amount: 3n }],
      groups: [
        ...solGroup,
        { address: SOL_INTERFACE, role: WRITE },
        { address: key(36), role: WRITE },
      ],
    });
    expect(await verdict(terms([sol(5n)]), bytes)).toBe("Legs");
    const withdrawal: PublicLeg = {
      direction: "withdrawal",
      mint: null,
      amount: 3n,
      accounts: [SOL_INTERFACE, key(36)],
    };
    expect(await verdict(terms([sol(5n), withdrawal]), bytes)).toBe("Accepted");
  });

  it("refuses a leg of another direction, mint or settlement account", async () => {
    const bytes = await message({ transfers: [solLeg(5n)], groups: solGroup });
    for (const changed of [
      { direction: "withdrawal" as const },
      { mint: MINT },
      { accounts: [key(34), FUNDER] },
      { accounts: [...sol(5n).accounts, key(34)] },
    ])
      expect(
        await verdict({ ...terms([sol(5n)]), legs: [{ ...sol(5n), ...changed }] }, bytes),
      ).toBe("Legs");
  });

  it("refuses a leg another key funds, the frame holds no leg of the signer", async () => {
    const bytes = await message({
      transfers: [solLeg(5n)],
      groups: [solGroup[0] as AccountMeta, { address: key(35), role: SIGN_WRITE }],
    });
    expect(await verdict(terms([{ ...sol(5n), accounts: [key(30), key(35)] }]), bytes)).toBe(
      "Accounts",
    );
  });

  it("refuses a key the frame also names in the owner run", async () => {
    const bytes = await message({
      transfers: [solLeg(5n)],
      groups: solGroup,
      run: [{ address: FUNDER, role: AccountRole.READONLY_SIGNER }],
    });
    expect(await verdict(terms([sol(5n)]), bytes)).toBe("Accounts");
  });

  it("refuses a writable account in the ring prefix ahead of SPP", async () => {
    const bytes = await message({
      transfers: [solLeg(5n)],
      groups: solGroup,
      registry: { address: POLICY.tracking, role: WRITE },
    });
    expect(await verdict(terms([sol(5n)]), bytes)).toBe("Accounts");
  });

  it("refuses a writable funder signer when terms fund only SPL legs", async () => {
    const splLeg = { kind: "splDeposit" as const, amount: 5n, splInterfaceBump: 255 };
    const splGroup = (role: AccountRole): readonly AccountMeta[] => [
      { address: MINT, role: READ },
      { address: key(31), role: WRITE },
      { address: FUNDER, role },
      { address: key(32), role: WRITE },
      { address: key(33), role: READ },
    ];
    const readonlyFunder = await message({
      transfers: [splLeg],
      groups: splGroup(AccountRole.READONLY_SIGNER),
    });
    const spl: PublicLeg = {
      direction: "deposit",
      mint: MINT,
      amount: 5n,
      accounts: splGroup(READ).map((meta) => meta.address),
    };
    expect(await verdict(terms([spl]), readonlyFunder)).toBe("Accepted");
    const writableFunder = await message({
      transfers: [splLeg],
      groups: splGroup(SIGN_WRITE),
    });
    expect(await verdict(terms([spl]), writableFunder)).toBe("Accounts");
  });

  it("reads terms with no leg and another signer as unrequested", async () => {
    const bytes = await message({ transfers: [solLeg(5n)], groups: solGroup });
    expect(await verdict(terms([]), bytes)).toBe("Unrequested");
    expect(await verdict(terms([sol(5n)]), bytes, key(9))).toBe("Unrequested");
  });

  it("encodes its terms as Rust serde, null on a SOL leg and no tag on an exit slot", () => {
    const named = terms(
      [
        sol(5n),
        {
          direction: "deposit",
          mint: MINT,
          amount: (1n << 60n) + 1n,
          accounts: [MINT, key(31), FUNDER, key(32), key(33)],
        },
      ],
      [SLOTS[0] as RelayFunderTerms["slots"][number], { hash: field(91) }],
    );
    const text = encodeRelayFunderTerms(named);
    expect(text).toContain(
      `"externalDataHash":"${encodeBase58(named.externalDataHash)}","slots":[{"hash":"${encodeBase58(field(10))}","tag":"${encodeBase58(field(40))}"},{"hash":"${encodeBase58(field(91))}"}],"legs":`,
    );
    expect(text).toContain(
      `{"direction":"deposit","mint":null,"amount":5,"accounts":["${key(30)}","${FUNDER}"]}`,
    );
    expect(text).toContain(`"amount":${(1n << 60n) + 1n},`);
    expect(decodeRelayFunderTerms(text)).toEqual(named);
    expect(decodeRelayFunderTerms(text.replace('}],"legs"', ',"tag":null}],"legs"'))).toEqual(
      named,
    );
    for (const [from, to] of [
      ['"externalDataHash"', '"hash"'],
      ['"slots"', '"slats"'],
      ['{"hash"', '{"commitment"'],
      ['"tag":"', '"blinding":"'],
      ['"legs"', '"lags"'],
      ['"direction":"deposit","mint":null,', '"mint":null,'],
      ['"direction":"deposit","mint":null', '"direction":"refund","mint":null'],
      ['"mint":null,', '"mint":null,"funder":null,'],
      ['"mint":null', '"mint":7'],
    ] as const)
      expect(() => decodeRelayFunderTerms(text.replace(from, to))).toThrow(
        "RING_RELAY_MESSAGE_INVALID",
      );
  });
});

describe("party cells", () => {
  const zero = new Uint8Array(32) as Bytes32;

  it("seats each co-signer in account order and no payer or funder", () => {
    const co = key(5);
    const cells = anonymousPartyIdentities([co]);
    expect(cells).toHaveLength(5);
    expect(cells[0]).not.toEqual(zero);
    expect(cells.slice(1)).toEqual(Array(4).fill(zero));
    expect(anonymousPartyIdentities([])).toEqual(Array(5).fill(zero));
  });

  it("refuses co-signers past the cells", () => {
    expect(() => anonymousPartyIdentities([40, 41, 42, 43, 44].map(key))).not.toThrow();
    expect(() => anonymousPartyIdentities([40, 41, 42, 43, 44, 45].map(key))).toThrow();
  });
});
