import { readFileSync } from "node:fs";

import { address } from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { initializePoseidon } from "@heliuslabs/spr";
import type { Bytes32, Bytes64 } from "@heliuslabs/spr/interface";
import { encodeBase58 } from "@heliuslabs/spr/internal";

import {
  ENROLLMENT_SIGNATURE_DOMAIN,
  SIGNED_MESSAGE_MAX_LEN,
  encodeEnrollmentWrite,
  signedWrite,
  type EnrollmentWrite,
} from "../../../src/protocol/ring-rpc/enrollment-write.js";

interface SignatureCase {
  readonly registry: string;
  readonly write: string;
  readonly entry_hash: string;
  readonly message: string;
}

const vector = JSON.parse(
  readFileSync(
    new URL("../../../../../test-vectors/rpc/enrollment_signature.json", import.meta.url),
    "utf8",
  ),
) as Readonly<{ owner: string; cases: readonly SignatureCase[] }>;

/** Rust `EnrollmentWrite::parse` of one whole write, `owner || kind || body`. */
function parsed(hex: string): EnrollmentWrite {
  const bytes = Uint8Array.from(Buffer.from(hex, "hex"));
  const at = (from: number, length: number) => bytes.slice(from, from + length);
  const owner = address(encodeBase58(at(0, 32)));
  if (bytes[32] === 1)
    return {
      owner,
      change: {
        kind: "rotate",
        viewingU: at(33, 32) as Bytes32,
        priorViewingU: at(65, 32) as Bytes32,
        priorSeq: new DataView(at(97, 8).buffer).getBigUint64(0, true),
      },
    };
  return {
    owner,
    change: {
      kind: "enroll",
      record: {
        nullifierPk: at(33, 32) as Bytes32,
        listTag: at(65, 32) as Bytes32,
        viewingU: at(97, 32) as Bytes32,
        ephU: at(129, 32) as Bytes32,
        ciphertext: at(161, 64) as Bytes64,
      },
      proof: at(225, 192),
    },
  };
}

beforeAll(initializePoseidon);

describe("enrollment_signature.json", () => {
  it("builds each owner's text byte for byte as Rust", () => {
    expect(vector.cases.map(({ message }) => message.split("\n").at(-1))).toEqual([
      "action: enroll",
      "action: rotate",
      "action: enroll",
    ]);
    for (const { registry, write: hex, entry_hash, message } of vector.cases) {
      const write = parsed(hex);
      expect(write.owner).toBe(vector.owner);
      expect(Buffer.from(encodeEnrollmentWrite(write)).toString("hex")).toBe(hex);
      const signed = signedWrite(write, address(registry));
      expect(encodeBase58(signed.entryHash)).toBe(entry_hash);
      expect(signed.message).toEqual(new TextEncoder().encode(message));
      expect(message.startsWith(`${ENROLLMENT_SIGNATURE_DOMAIN}\nregistry: ${registry}\n`)).toBe(
        true,
      );
      expect(message).toMatch(/^[ -~\n]+$/);
      expect(signed.message.length).toBeLessThanOrEqual(SIGNED_MESSAGE_MAX_LEN);
    }
  });
});
