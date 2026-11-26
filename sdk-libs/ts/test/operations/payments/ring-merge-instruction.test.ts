import { address, isSignerRole } from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { initializePoseidon } from "@heliuslabs/spr";
import { ZERO_32, ZERO_NULLIFIER_PK } from "@heliuslabs/spr/internal";
import { MergeEncryptionScheme } from "@heliuslabs/spr/interface";
import type { Bytes16, Bytes32, Bytes33, Bytes128 } from "@heliuslabs/spr/interface";
import { ShieldedAddress, ShieldedKeypair } from "@heliuslabs/spr/keypair";
import { Wallet } from "@heliuslabs/spr/transaction";

import {
  buildRingMergeTransaction,
  ringMergeInstruction,
  type RingMergeTransactionParams,
} from "../../../src/operations/payments/merge.js";
import { decodeRingMergeRequest } from "../../../src/crypto/audit/merge-disclosure.js";
import {
  MERGE_ENVELOPE_LEN,
  MERGE_RECORD_LEN,
} from "../../../src/protocol/codec/ring-disclosure-layout.js";
import { CUSTOM_RING_PROOF_LENGTH } from "../../../src/protocol/codec/ring-codecs.js";
import { RingInstructionTag } from "../../../src/protocol/transaction/instruction-tag.js";

const RING = address("9vyTbYGyh3cwxkAQpjjFQGXmdJP6p9B6YcQ5pNuXPNbh");
const TREE = address("4Nd1mBQtrMJVYVfKf2PJy9NZUZdTAsp7D4xWLs4gDB4T");
const TRACKING = address("Stor111111111111111111111111111111111111111");
const PAYER = address("So11111111111111111111111111111111111111112");

/** A compressible G2 point, the generator's bytes. */
const G2 = Uint8Array.from(
  Buffer.from(
    "198e9393920d483a7260bfb731fb5d25f1aa493335a9e71297e485b7aef312c2" +
      "1800deef121f1e76426a00665e5c4479674322d4f75edadd46debd5cd992f6ed" +
      "090689d0585ff075ec9e99ad690c3395bc4b313370b38ef355acdadcd122975b" +
      "12c85ea5db8c6deb4aab71808dcb408fe3d1e7690c43d37b4ce6cc0166fa7daa",
    "hex",
  ),
) as Bytes128;

const data = () => ({
  expiryUnixTs: 9n,
  proof: { a: ZERO_32, b: G2, c: ZERO_32 },
  outputUtxoHash: new Uint8Array(32).fill(1) as Bytes32,
  outputEncryption: {
    scheme: MergeEncryptionScheme.PoseidonCtrV1,
    salt: new Uint8Array(16) as Bytes16,
    ciphertext: [ZERO_32, ZERO_32, ZERO_32] as const,
  },
  privateTxHash: new Uint8Array(32).fill(2) as Bytes32,
  nullifiers: [new Uint8Array(32).fill(3) as Bytes32, new Uint8Array(32).fill(4) as Bytes32],
  utxoTreeRootIndex: 0,
  nullifierTreeRootIndex: 0,
});

const sealed = {
  proof: new Uint8Array(CUSTOM_RING_PROOF_LENGTH),
  ephemeralPublicKey: new Uint8Array(33) as Bytes33,
  envelope: new Uint8Array(MERGE_ENVELOPE_LEN).fill(4),
  record: new Uint8Array(MERGE_RECORD_LEN).fill(5),
};

async function instruction() {
  return ringMergeInstruction({
    ringProgramId: RING,
    inputTree: TREE,
    outputTree: TREE,
    payer: PAYER,
    data: data(),
    outputRingDataHash: ZERO_32,
    sealed,
    tracking: TRACKING,
    trackingRevision: 7n,
  });
}

describe("ring merge tag 57", () => {
  beforeAll(initializePoseidon);

  it("binds the tracking revision and names no owner signer", async () => {
    const built = await instruction();
    const bytes = Uint8Array.from(built.data ?? []);
    expect(bytes[0]).toBe(RingInstructionTag.merge);
    const request = decodeRingMergeRequest(bytes.subarray(1));
    expect(request.trackingRevision).toBe(7n);
    expect(request.outputRingDataHash).toEqual(ZERO_32);
    expect(request.sealed).toEqual(sealed);
    expect(built.accounts?.[1]?.address).toBe(TRACKING);
    expect(built.accounts).toHaveLength(8 + 2);
    expect(
      built.accounts?.filter(({ role }) => isSignerRole(role)).map((account) => account.address),
    ).toEqual([PAYER]);
  });

  it("refuses an owner under the zero nullifier key before any read", async () => {
    const owner = ShieldedKeypair.generate();
    const held = owner.shieldedAddress();
    const identity = ShieldedAddress.fromPublicKeys(
      held.signingPublicKey,
      ZERO_NULLIFIER_PK,
      held.viewingPublicKey,
    );
    const params = {
      client: {},
      ringProgramId: RING,
      wallet: new Wallet({ identity }),
      keys: { address: () => identity, viewingPublicKeys: () => [identity.viewingPublicKey] },
      feePayer: PAYER,
    } as unknown as RingMergeTransactionParams;
    await expect(buildRingMergeTransaction(params)).rejects.toMatchObject({
      code: "RING_BUILD_MERGE",
      causeCode: "RING_ZERO_KEY_MERGE_UNAVAILABLE",
    });
    owner.destroy();
  });
});
