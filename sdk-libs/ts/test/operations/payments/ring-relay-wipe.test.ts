import { getBase58Decoder, type Address } from "@solana/kit";
import { expect, it, vi } from "vitest";

import { initializePoseidon } from "@heliuslabs/spr";
import type { Bytes32 } from "@heliuslabs/spr/interface";
import { ShieldedKeypair, SigningKey } from "@heliuslabs/spr/keypair";

import { PreparedRingDeposit } from "../../../src/operations/payments/deposit.js";
import { BLOCKHASH } from "../../../../../zolana/sdk-libs/ts/test/helpers/clients.js";
import {
  PROOF_SERVICE,
  freshWalletTags,
  ringFixture,
  walletKeys,
} from "../../helpers/ring-pipeline.js";

const seen = vi.hoisted(() => ({ payloads: [] as Uint8Array[] }));

vi.mock("../../../src/protocol/ring-rpc/relay-proof-inputs.js", async (original) => {
  const actual =
    await original<typeof import("../../../src/protocol/ring-rpc/relay-proof-inputs.js")>();
  return {
    ...actual,
    encodeRelayProofInputs: (value: Parameters<typeof actual.encodeRelayProofInputs>[0]) => {
      const payload = actual.encodeRelayProofInputs(value);
      seen.payloads.push(payload);
      return payload;
    },
  };
});

vi.mock("../../../src/proof/policy-proof.js", async (original) => ({
  ...(await original<typeof import("../../../src/proof/policy-proof.js")>()),
  chargedOutflowsOf: () => {
    throw new Error("outflows");
  },
}));

it("wipes a deposit frame's encoded payload when the frame fails after encoding", async () => {
  await initializePoseidon();
  const fixture = await ringFixture({ mode: "anonymous" });
  const owner = ShieldedKeypair.generate();
  fixture.enroll(owner.shieldedAddress());
  const signing = SigningKey.generate();
  const funder = getBase58Decoder().decode(signing.publicKey().ed25519()) as Address;
  await expect(
    PreparedRingDeposit.prepareRelay({
      client: {
        ...fixture.client,
        proofDataSource: "prover",
        proofService: PROOF_SERVICE,
        getLatestBlockhash: async () => BLOCKHASH,
      },
      ringProgramId: fixture.ringProgramId,
      depositor: funder,
      depositorSigner: { address: funder } as never,
      recipient: {
        kind: "anonymous",
        keys: walletKeys(owner),
        listSalt: new Uint8Array(32).fill(1) as Bytes32,
      },
      tags: await freshWalletTags(owner, [owner]),
      amount: 9n,
      ringRpc: {
        cluster: new Uint8Array(32).fill(1) as Bytes32,
        service: getBase58Decoder().decode(new Uint8Array(32).fill(2)) as Address,
      },
      quote: { feePayer: fixture.authority, validUntilSlot: 100n },
    }),
  ).rejects.toMatchObject({ code: "RING_BUILD_DEPOSIT" });
  const [payload] = seen.payloads;
  expect(seen.payloads).toHaveLength(1);
  expect(payload?.length).toBeGreaterThan(0);
  expect(payload?.every((byte) => byte === 0)).toBe(true);
});
