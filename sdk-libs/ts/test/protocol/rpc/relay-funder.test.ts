/** A public funder signs for its own deposit legs and nothing else of its. */
import { AccountRole, type AccountMeta, type Address } from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { initializePoseidon } from "@heliuslabs/spr";
import type { Bytes32 } from "@heliuslabs/spr/interface";
import { compileUnsignedTransaction, encodeBase58 } from "@heliuslabs/spr/internal";

import { ringComputeUnitLimit } from "../../../src/operations/management/admin.js";
import { SIZE_PROBE_LIFETIME } from "../../../src/operations/workflow/compile.js";
import { FunderApproval } from "../../../src/proof/cosign.js";
import { PendingRelaySignature } from "../../../src/rpc/relay.js";
import { anonymousPartyIdentities } from "../../../src/protocol/membership/parties.js";
import { ringAuthAddress, ringPolicyAddress } from "../../../src/protocol/pda/index.js";
import { RingInstructionTag } from "../../../src/protocol/transaction/instruction-tag.js";
import {
  checkRelayMessage,
  decodeRelayFunderTerms,
  encodeRelayFunderTerms,
  expectedFunder,
  type RelayFunderTerms,
} from "../../../src/rpc/relay-message.js";
import type { RingTransactData } from "../../../src/protocol/transaction/transact.js";
import { ringFrame, ringInstructionData } from "../../helpers/ring-frame.js";

const key = (byte: number): Address => encodeBase58(new Uint8Array(32).fill(byte));
const RING = key(1);
const PAYER = key(2);
const FUNDER = key(3);
const MINT = key(4);
const READ = AccountRole.READONLY;
const WRITE = AccountRole.WRITABLE;
const SIGN_WRITE = AccountRole.WRITABLE_SIGNER;
const RELAY_ID = "11111111111111111111111111111111" as never;

beforeAll(initializePoseidon);

const terms = (legs: RelayFunderTerms["legs"]): RelayFunderTerms => ({
  ring: RING,
  feePayer: PAYER,
  signer: FUNDER,
  legs,
  setup: [],
});

/** The ring account, SPP's head, one tree, then the legs' settlement groups. */
async function message(
  input: Readonly<{
    transfers: RingTransactData["interfaceTransfers"];
    groups: readonly AccountMeta[];
    run?: readonly AccountMeta[];
  }>,
): Promise<Uint8Array> {
  const frame: RingTransactData = {
    ...ringFrame({ rail: "confidential", inputs: 1, outputs: 2 }),
    interfaceTransfers: input.transfers,
  };
  const spp = (at: number, role: AccountRole): AccountMeta => ({ address: key(at), role });
  const accounts: AccountMeta[] = [
    { address: await ringPolicyAddress(RING), role: READ },
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
  const expected = await expectedFunder({ terms: named, config: { mode: "confidential" } });
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

describe("a public funder's check", () => {
  it("passes the legs its terms name", async () => {
    const bytes = await message({ transfers: [solLeg(5n)], groups: solGroup });
    expect(await verdict(terms([{ amount: 5n }]), bytes)).toBe("Accepted");
    const pending = await new FunderApproval({
      relayId: RELAY_ID,
      message: bytes,
      terms: terms([{ amount: 5n }]),
      config: { mode: "confidential" },
    }).pending();
    expect(pending.verify(FUNDER).signer).toBe(FUNDER);
  });

  it("signs a sealed deposit frame through the funder's own terms and the returned message", async () => {
    const bytes = await message({ transfers: [solLeg(5n)], groups: solGroup });
    const named = terms([{ amount: 5n }]);
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
      config: { mode: "confidential" },
    }).pending();
    expect(pending.verify(FUNDER).signer).toBe(FUNDER);
  });

  it("refuses another amount, a missing leg and a leg it did not name", async () => {
    const one = await message({ transfers: [solLeg(5n)], groups: solGroup });
    expect(await verdict(terms([{ amount: 6n }]), one)).toBe("FunderLegs");
    expect(await verdict(terms([{ amount: 5n }, { amount: 7n }]), one)).toBe("FunderLegs");
    const two = await message({
      transfers: [solLeg(5n), solLeg(7n)],
      groups: [...solGroup, { address: key(30), role: WRITE }, solGroup[1] as AccountMeta],
    });
    expect(await verdict(terms([{ amount: 5n }]), two)).toBe("FunderLegs");
  });

  it("refuses a key the frame also names in the owner run", async () => {
    const bytes = await message({
      transfers: [solLeg(5n)],
      groups: solGroup,
      run: [{ address: FUNDER, role: AccountRole.READONLY_SIGNER }],
    });
    expect(await verdict(terms([{ amount: 5n }]), bytes)).toBe("Accounts");
  });

  it("refuses a writable account in the ring prefix ahead of SPP", async () => {
    const frame: RingTransactData = {
      ...ringFrame({ rail: "confidential", inputs: 1, outputs: 2 }),
      interfaceTransfers: [solLeg(5n)],
    };
    const spp = (at: number, role: AccountRole): AccountMeta => ({ address: key(at), role });
    const accounts: AccountMeta[] = [
      { address: await ringPolicyAddress(RING), role: READ },
      { address: key(99), role: WRITE },
      { address: PAYER, role: SIGN_WRITE },
      spp(10, WRITE),
      spp(11, READ),
      spp(12, READ),
      { address: await ringAuthAddress(RING), role: READ },
      spp(13, WRITE),
      ...frame.nullifiers.map((_, at) => spp(20 + at, WRITE)),
      ...solGroup,
    ];
    const { messageBytes } = compileUnsignedTransaction({
      feePayer: PAYER,
      lifetime: SIZE_PROBE_LIFETIME,
      instructions: [{ programAddress: RING, accounts, data: ringInstructionData(frame) }],
      computeUnitLimit: ringComputeUnitLimit(RingInstructionTag.transfer),
    });
    expect(await verdict(terms([{ amount: 5n }]), new Uint8Array(messageBytes))).toBe("Accounts");
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
    expect(await verdict(terms([{ mint: MINT, amount: 5n }]), readonlyFunder)).toBe("Accepted");
    const writableFunder = await message({
      transfers: [splLeg],
      groups: splGroup(SIGN_WRITE),
    });
    expect(await verdict(terms([{ mint: MINT, amount: 5n }]), writableFunder)).toBe("Accounts");
  });

  it("reads terms with no leg and another signer as unrequested", async () => {
    const bytes = await message({ transfers: [solLeg(5n)], groups: solGroup });
    expect(await verdict(terms([]), bytes)).toBe("Unrequested");
    expect(await verdict(terms([{ amount: 5n }]), bytes, key(9))).toBe("Unrequested");
  });

  it("encodes its terms as Rust serde, null on a SOL leg", () => {
    const named = terms([{ amount: 5n }, { mint: MINT, amount: (1n << 60n) + 1n }]);
    const text = encodeRelayFunderTerms(named);
    expect(text).toContain('"mint":null');
    expect(encodeRelayFunderTerms(decodeRelayFunderTerms(text))).toBe(text);
    expect(() => decodeRelayFunderTerms(text.replace('"legs"', '"lags"'))).toThrow();
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
