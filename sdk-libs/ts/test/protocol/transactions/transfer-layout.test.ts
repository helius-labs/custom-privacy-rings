import { describe, expect, it } from "vitest";

import vector from "../../../../../test-vectors/proof/transfer-layout.json" with { type: "json" };
import { decodeRingPolicy } from "../../../src/protocol/codec/ring-codecs.js";
import { RingTransactData, type RingRail } from "../../../src/protocol/transaction/transact.js";
import {
  coSignerPairs,
  knownPrefixEndingAt,
  settlementLegs,
  transferLayout,
  transferRegistryCount,
  type AccountWindow,
} from "../../../src/protocol/transaction/transfer-layout.js";

const bytes = (hex: string): Uint8Array => Uint8Array.from(Buffer.from(hex, "hex"));
const span = ({ start, end }: AccountWindow): number[] => [start, end];

describe("transfer account layout", () => {
  it.each(vector.cases)("cuts the windows Rust cuts, $name", (entry) => {
    const frame = RingTransactData.parse(bytes(entry.data).subarray(1), entry.rail as RingRail);
    const ring = transferRegistryCount(decodeRingPolicy(bytes(entry.ring_policy)), frame.rail);
    const layout = transferLayout(frame, ring, entry.accounts.length, {
      coSignerPairs: entry.co_signer_pairs,
      delegateMint: entry.delegate_mint,
    });
    expect({
      ring: 0,
      registries: span(layout.registries),
      co_signers: span(layout.coSigners),
      delegate: layout.delegate ?? null,
      mint: layout.mint ?? null,
      spp: layout.spp,
      payer: layout.payer,
      ring_config: layout.ringConfig,
      trees: span(layout.trees),
      nullifiers: span(layout.nullifiers),
      owners: span(layout.owners),
      settlement: span(layout.settlement),
      cache: span(layout.cache),
      legs: settlementLegs(layout, frame.interfaceTransfers).map((leg) => ({
        accounts: span(leg.accounts),
        funder: leg.funder ?? null,
      })),
    }).toEqual(entry.windows);
    expect(coSignerPairs(layout)).toHaveLength(entry.co_signer_pairs);
    expect(knownPrefixEndingAt(frame, ring, entry.windows.spp)).toEqual({
      coSignerPairs: entry.co_signer_pairs,
      delegateMint: entry.delegate_mint,
    });
  });

  it("refuses a list too short for the frame's counts and an SPP window no prefix ends at", () => {
    const [entry] = vector.cases;
    if (entry === undefined) throw new Error("no vector case");
    const frame = RingTransactData.parse(bytes(entry.data).subarray(1), entry.rail as RingRail);
    const ring = transferRegistryCount(decodeRingPolicy(bytes(entry.ring_policy)), frame.rail);
    const prefix = { coSignerPairs: 0, delegateMint: false };
    const shortest = entry.windows.owners[0] ?? 0;
    expect(transferLayout(frame, ring, shortest, prefix).owners).toEqual({
      start: shortest,
      end: shortest,
    });
    expect(() => transferLayout(frame, ring, shortest - 1, prefix)).toThrow(
      "RING_SETTLEMENT_ACCOUNTS_INVALID",
    );
    for (const spp of [entry.windows.spp - 1, entry.windows.spp + 1])
      expect(() => knownPrefixEndingAt(frame, ring, spp)).toThrow(
        "RING_SETTLEMENT_ACCOUNTS_INVALID",
      );
  });
});
