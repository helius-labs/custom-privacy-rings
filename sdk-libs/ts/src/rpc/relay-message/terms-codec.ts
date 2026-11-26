import { address } from "@solana/kit";
import { encodeBase58 } from "@heliuslabs/spr/internal";
import { wirePrimitives } from "../../protocol/wire-primitives.js";
import type {
  ApprovalOpening,
  OutputTotalsOpening,
  PublicTotalsOpening,
} from "../../protocol/membership/approval-totals.js";
import {
  type RelayTransferTerms,
  type PublicLeg,
  mismatch,
  type TokenAccountCreate,
  withMismatch,
  type RelayFunderTerms,
  type SlotOpening,
} from "./expected.js";

/** The engine floor holds `JSON.rawJSON`, the TS lib does not type it. */
const rawJson = (JSON as unknown as { rawJSON(text: string): unknown }).rawJSON;

/** Rust's serde JSON byte for byte, an amount past the safe integers stays exact. */
export function encodeRelayTransferTerms(terms: RelayTransferTerms): string {
  return JSON.stringify({
    ring: terms.ring,
    feePayer: terms.feePayer,
    signer: terms.signer,
    slots: terms.slots.map((opening) => ({
      slot: opening.slot,
      owner: opening.owner,
      nullifierPk: encodeBase58(opening.nullifierPk),
      mint: opening.mint,
      amount: rawJson(opening.amount.toString()),
      blinding: encodeBase58(opening.blinding),
      treeId: opening.treeId,
      dataHash: encodeBase58(opening.dataHash),
      ringDataHash: encodeBase58(opening.ringDataHash),
      ...(opening.tag === undefined ? {} : { tag: encodeBase58(opening.tag) }),
    })),
    nullifiers: terms.nullifiers.map(encodeBase58),
    legs: terms.legs.map(encodeLeg),
    setup: terms.setup,
  });
}

function encodeLeg(leg: PublicLeg): unknown {
  return {
    direction: leg.direction,
    mint: leg.mint,
    amount: rawJson(leg.amount.toString()),
    accounts: leg.accounts,
  };
}

/** An integer past the safe range keeps its digits, Serde refuses `7.0` and `-0` for an integer. */
function parseWire(json: string): unknown {
  return JSON.parse(json, (_key: string, value: unknown, context?: { source?: string }) => {
    if (typeof value !== "number") return value;
    const source = context?.source ?? "";
    if (!/^(0|[1-9][0-9]*)$/.test(source)) throw mismatch("Undecodable");
    return Number.isSafeInteger(value) ? value : source;
  }) as unknown;
}

function decodeTokenAccountCreate(entry: unknown): TokenAccountCreate {
  const create = fields(entry, ["owner", "mint", "tokenProgram"]);
  return Object.freeze({
    owner: address(create["owner"] as string),
    mint: address(create["mint"] as string),
    tokenProgram: address(create["tokenProgram"] as string),
  });
}

export function decodeRelayTransferTerms(json: string): RelayTransferTerms {
  return withMismatch("Undecodable", () => {
    const wire = parseWire(json);
    const row = fields(wire, [
      "ring",
      "feePayer",
      "signer",
      "slots",
      "nullifiers",
      "legs",
      "setup",
    ]);
    return Object.freeze({
      ring: address(row["ring"] as string),
      feePayer: address(row["feePayer"] as string),
      signer: address(row["signer"] as string),
      slots: (row["slots"] as readonly unknown[]).map(decodeSlotOpening),
      nullifiers: (row["nullifiers"] as readonly unknown[]).map((nullifier) =>
        scalar.hash(nullifier, "nullifiers"),
      ),
      legs: (row["legs"] as readonly unknown[]).map(decodeLeg),
      setup: (row["setup"] as readonly unknown[]).map(decodeTokenAccountCreate),
    });
  });
}

/** Rust's serde JSON byte for byte. */
export function encodeRelayFunderTerms(terms: RelayFunderTerms): string {
  return JSON.stringify({
    ring: terms.ring,
    feePayer: terms.feePayer,
    signer: terms.signer,
    legs: terms.legs.map(encodeLeg),
    setup: terms.setup,
  });
}

export function decodeRelayFunderTerms(json: string): RelayFunderTerms {
  return withMismatch("Undecodable", () => {
    const row = fields(parseWire(json), ["ring", "feePayer", "signer", "legs", "setup"]);
    return Object.freeze({
      ring: address(row["ring"] as string),
      feePayer: address(row["feePayer"] as string),
      signer: address(row["signer"] as string),
      legs: (row["legs"] as readonly unknown[]).map(decodeLeg),
      setup: (row["setup"] as readonly unknown[]).map(decodeTokenAccountCreate),
    });
  });
}

function decodeLeg(entry: unknown): PublicLeg {
  const leg = fields(entry, ["direction", "mint", "amount", "accounts"]);
  const direction = leg["direction"];
  if (direction !== "deposit" && direction !== "withdrawal") throw mismatch("Undecodable");
  return Object.freeze({
    direction,
    mint: leg["mint"] === null ? null : address(leg["mint"] as string),
    amount: scalar.u64(leg["amount"], "amount"),
    accounts: Object.freeze(
      (leg["accounts"] as readonly unknown[]).map((account) => address(account as string)),
    ),
  });
}

function decodeSlotOpening(value: unknown): SlotOpening {
  const row = fields(
    value,
    [
      "slot",
      "owner",
      "nullifierPk",
      "mint",
      "amount",
      "blinding",
      "treeId",
      "dataHash",
      "ringDataHash",
    ],
    ["tag"],
  );
  return Object.freeze({
    slot: scalar.count(row["slot"], "slot", 0xffff_ffff),
    owner: address(row["owner"] as string),
    nullifierPk: scalar.hash(row["nullifierPk"], "nullifierPk"),
    mint: address(row["mint"] as string),
    amount: scalar.u64(row["amount"], "amount"),
    blinding: scalar.hash(row["blinding"], "blinding"),
    treeId: scalar.count(row["treeId"], "treeId", 0xffff),
    dataHash: scalar.hash(row["dataHash"], "dataHash"),
    ringDataHash: scalar.hash(row["ringDataHash"], "ringDataHash"),
    ...((row["tag"] ?? null) === null ? {} : { tag: scalar.hash(row["tag"], "tag") }),
  });
}

/** Rust `deny_unknown_fields`, every field of `names` required and each of `optional` a serde `default`. */
function fields(
  value: unknown,
  names: readonly string[],
  optional: readonly string[] = [],
): Readonly<Record<string, unknown>> {
  if (typeof value !== "object" || value === null) throw mismatch("Undecodable");
  const present = Object.keys(value);
  if (
    !names.every((name) => present.includes(name)) ||
    !present.every((name) => names.includes(name) || optional.includes(name))
  )
    throw mismatch("Undecodable");
  return value as Readonly<Record<string, unknown>>;
}

const scalar = wirePrimitives(() => {
  throw mismatch("Undecodable");
});

/** Rust serde JSON byte for byte, `RelayTransferTerms` hands nothing of it over. */
export function encodeApprovalOpening(opening: ApprovalOpening): string {
  return JSON.stringify({
    blinder: encodeBase58(opening.blinder),
    outputs: opening.outputs.map((slot) => ({
      asset: encodeBase58(slot.asset),
      peak: slot.peak,
      chargedOutflow: slot.chargedOutflow,
    })),
    public: opening.public.map((slot) => ({
      asset: encodeBase58(slot.asset),
      peak: slot.peak,
      outflow: slot.outflow,
      gross: slot.gross,
    })),
  });
}

export function decodeApprovalOpening(json: string): ApprovalOpening {
  return withMismatch("Undecodable", () => decodedApprovalOpening(JSON.parse(json)));
}

/** Rust `deny_unknown_fields`, an amount reads as text and is parsed at the check. */
function decodedApprovalOpening(wire: unknown): ApprovalOpening {
  const row = fields(wire, ["blinder", "outputs", "public"]);
  return Object.freeze({
    blinder: scalar.hash(row["blinder"], "blinder"),
    outputs: Object.freeze(
      (row["outputs"] as readonly unknown[]).map((entry): OutputTotalsOpening => {
        const slot = fields(entry, ["asset", "peak", "chargedOutflow"]);
        return Object.freeze({
          asset: scalar.hash(slot["asset"], "asset"),
          peak: scalar.text(slot["peak"], "peak"),
          chargedOutflow: scalar.text(slot["chargedOutflow"], "chargedOutflow"),
        });
      }),
    ),
    public: Object.freeze(
      (row["public"] as readonly unknown[]).map((entry): PublicTotalsOpening => {
        const slot = fields(entry, ["asset", "peak", "outflow", "gross"]);
        return Object.freeze({
          asset: scalar.hash(slot["asset"], "asset"),
          peak: scalar.text(slot["peak"], "peak"),
          outflow: scalar.text(slot["outflow"], "outflow"),
          gross: scalar.text(slot["gross"], "gross"),
        });
      }),
    ),
  });
}
