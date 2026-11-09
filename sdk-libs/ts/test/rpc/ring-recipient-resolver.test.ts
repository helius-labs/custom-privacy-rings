import { address, type Address } from "@solana/kit";

import { beforeAll, describe, expect, it } from "vitest";

import { initializePoseidon } from "@heliuslabs/spr";
import { poseidon, ZERO_32, ZERO_NULLIFIER_PK } from "@heliuslabs/spr/internal";
import type { Bytes32 } from "@heliuslabs/spr/interface";
import { ShieldedAddress, ShieldedKeypair, SigningKey } from "@heliuslabs/spr/keypair";

import { RingAddress } from "../../src/crypto/keys/ring-address.js";
import { readOutputKeys } from "../../src/operations/workflow/ring-reads.js";
import { checkOutputKeys } from "../../src/proof/policy-proof.js";
import type { ProofOutputUtxo } from "@heliuslabs/spr/transaction";
import { listSaltOf, listTagOf } from "../../src/crypto/keys/list-salt.js";
import type { TrackingSeal } from "../../src/protocol/indexer/tracking-codec.js";
import { QUAD_HEIGHT } from "../../src/protocol/membership/member-registry.js";
import {
  TRACKING_SENTINEL,
  trackingRecordHash,
  viewingKeyHash,
  type TrackingOpening,
  type TrackingRecord,
} from "../../src/protocol/membership/tracking.js";
import { resolveRingRecipient, type RingRecipientClient } from "../../src/rpc/tracking-resolver.js";
import { trackingRegistryAccount } from "../helpers/ring-accounts.js";

const RING = address("9vyTbYGyh3cwxkAQpjjFQGXmdJP6p9B6YcQ5pNuXPNbh");
const REGISTRY = address("Stor111111111111111111111111111111111111111");

const holder = () =>
  ShieldedKeypair.fromKeypair(SigningKey.fromEd25519Bytes(new Uint8Array(32).fill(7) as Bytes32));

function zeroSiblings(): readonly (readonly Bytes32[])[] {
  const levels: Bytes32[][] = [];
  let zero: Bytes32 = ZERO_32;
  for (let level = 0; level < QUAD_HEIGHT; level += 1) {
    levels.push([zero, zero, zero]);
    zero = poseidon([zero, zero, zero, zero]) as Bytes32;
  }
  return levels;
}

/** The tree holding only `leaf` at index 0, served as `opening` with `seal` when present. */
function chain(
  leaf: TrackingRecord,
  kind: "present" | "absent",
  seal?: TrackingSeal,
): RingRecipientClient {
  const siblings = zeroSiblings();
  let root = trackingRecordHash(leaf);
  for (const level of siblings) root = poseidon([root, ...level]) as Bytes32;
  const registry = trackingRegistryAccount({
    root,
    revision: 0n,
    nextIndex: 1n,
    auditorPubkey: new Uint8Array(33),
  });
  const path = { index: 0, siblings };
  const opening: TrackingOpening =
    kind === "present" ? { kind, record: leaf, path } : { kind, low: leaf, path };
  return {
    getAccount: async (read: Address) => (read === REGISTRY ? registry : undefined),
    getTrackingOpenings: async () => ({
      slot: 1n,
      root,
      revision: 0n,
      openings: [seal === undefined ? opening : { ...opening, seal }],
    }),
  };
}

/** The leaf an enrolled `to` holds at `viewingU` and the seal the indexer serves beside it. */
function enrolledLeaf(to: RingAddress, viewingU: Bytes32 = to.recipient.viewingPublicKey.u()) {
  const row: TrackingRecord = {
    ...TRACKING_SENTINEL,
    member: to.recipient.signingPublicKey.ownerProofInputHash(),
    nullifierPk: to.recipient.nullifierPublicKey,
    listTag: listTagOf(to.listSalt),
    viewingHash: viewingKeyHash(viewingU),
  };
  const seal: TrackingSeal = {
    owner: to.recipient.solanaAddress(),
    viewingU,
    ephPk: new Uint8Array(33),
    ciphertext: new Uint8Array(64),
  };
  return { row, seal };
}

async function addressed() {
  const wallet = holder();
  const to = await RingAddress.enrolled(wallet, RING, listSaltOf(wallet.viewingKey(), RING));
  return { to, wallet, ...enrolledLeaf(to) };
}

beforeAll(initializePoseidon);

describe("resolveRingRecipient", () => {
  it("returns the row's key, tag and viewing key, never a salt", async () => {
    const { to, row, seal } = await addressed();
    const resolved = await resolveRingRecipient({
      client: chain(row, "present", seal),
      trackingRegistry: REGISTRY,
      to,
    });
    if (resolved.kind !== "enrolled") throw new Error("enrolled");
    expect(resolved.nullifierPk).toEqual(row.nullifierPk);
    expect(resolved.viewingPublicKey.u()).toEqual(to.recipient.viewingPublicKey.u());
    expect(resolved.listTag).toEqual(listTagOf(to.listSalt));
    expect(Object.keys(resolved)).not.toContain("listSalt");
  });

  it("refuses a key or salt the verified row does not hold", async () => {
    const { to, row, seal } = await addressed();
    for (const [name, wrong] of [
      ["nullifierPk", { nullifierPk: new Uint8Array(32).fill(5) as Bytes32 }],
      ["listSalt", { listTag: new Uint8Array(32).fill(5) as Bytes32 }],
    ] as const)
      await expect(
        resolveRingRecipient({
          client: chain({ ...row, ...wrong }, "present", seal),
          trackingRegistry: REGISTRY,
          to,
        }),
      ).rejects.toMatchObject({
        code: "RING_ADDRESS_MISMATCH",
        details: { field: name },
      });
  });

  it("resolves a keyed address with no row to its own keys, no zero key and no list", async () => {
    const { to } = await addressed();
    // No leaf names the owner, the absence opening alone resolves the address.
    const resolved = await resolveRingRecipient({
      client: chain(TRACKING_SENTINEL, "absent"),
      trackingRegistry: REGISTRY,
      to,
    });
    if (resolved.kind !== "unregistered") throw new Error("unregistered");
    expect(resolved.nullifierPk).toEqual(to.recipient.nullifierPublicKey);
    expect(resolved.nullifierPk).not.toEqual(ZERO_NULLIFIER_PK);
    expect(resolved.viewingPublicKey.u()).toEqual(to.recipient.viewingPublicKey.u());
    expect(Object.keys(resolved)).not.toContain("listTag");
  });

  it("resolves an address from before a rotation to the leaf's current viewing key", async () => {
    const { to } = await addressed();
    const rotated = ShieldedKeypair.generate().viewingPublicKey();
    const { row, seal } = enrolledLeaf(to, rotated.u());
    const resolved = await resolveRingRecipient({
      client: chain(row, "present", seal),
      trackingRegistry: REGISTRY,
      to,
    });
    if (resolved.kind !== "enrolled") throw new Error("enrolled");
    expect(resolved.viewingPublicKey.u()).toEqual(rotated.u());
    expect(resolved.viewingPublicKey.u()).not.toEqual(to.recipient.viewingPublicKey.u());
  });

  it("refuses a served viewing key the leaf does not hash, and a present leaf with no seal", async () => {
    const { to, row, seal } = await addressed();
    const rotated = ShieldedKeypair.generate().viewingPublicKey();
    await expect(
      resolveRingRecipient({
        client: chain(row, "present", { ...seal, viewingU: rotated.u() }),
        trackingRegistry: REGISTRY,
        to,
      }),
    ).rejects.toMatchObject({ code: "RING_VIEWING_KEY_MISMATCH" });
    await expect(
      resolveRingRecipient({ client: chain(row, "present"), trackingRegistry: REGISTRY, to }),
    ).rejects.toMatchObject({ code: "RING_TRACKING_INVALID", details: { reason: "seal" } });
  });

  describe("under the zero key", () => {
    const zeroAddressed = (wallet: ShieldedKeypair) => {
      const address_ = wallet.shieldedAddress();
      return new RingAddress({
        recipient: ShieldedAddress.fromPublicKeys(
          address_.signingPublicKey,
          ZERO_NULLIFIER_PK as Bytes32,
          address_.viewingPublicKey,
        ),
        ringProgramId: RING,
        listSalt: ZERO_32,
      });
    };

    it("admits an owner with no row under the address's viewing key", async () => {
      const to = zeroAddressed(holder());
      const resolved = await resolveRingRecipient({
        client: chain(TRACKING_SENTINEL, "absent"),
        trackingRegistry: REGISTRY,
        to,
      });
      expect(resolved.kind).toBe("zeroKey");
      expect(resolved.viewingPublicKey.u()).toEqual(to.recipient.viewingPublicKey.u());
    });

    it("resolves a rotated enrolled owner's old address to the leaf's current key", async () => {
      const { to, wallet } = await addressed();
      const rotated = ShieldedKeypair.generate().viewingPublicKey();
      const { row, seal } = enrolledLeaf(to, rotated.u());
      const resolved = await resolveRingRecipient({
        client: chain(row, "present", seal),
        trackingRegistry: REGISTRY,
        to: zeroAddressed(wallet),
      });
      expect(resolved.kind).toBe("zeroKey");
      expect(resolved.viewingPublicKey.u()).toEqual(rotated.u());
      await expect(
        resolveRingRecipient({
          client: chain(row, "present"),
          trackingRegistry: REGISTRY,
          to: zeroAddressed(wallet),
        }),
      ).rejects.toMatchObject({
        code: "RING_TRACKING_INVALID",
        details: { reason: "seal" },
      });
    });

    it("refuses a zero key payment to an enrolled owner after viewing key resolution", async () => {
      const { to, wallet } = await addressed();
      const rotated = ShieldedKeypair.generate().viewingPublicKey();
      const old = zeroAddressed(wallet);
      const { row, seal } = enrolledLeaf(to, rotated.u());
      const resolved = await resolveRingRecipient({
        client: chain(row, "present", seal),
        trackingRegistry: REGISTRY,
        to: old,
      });
      const paying = (viewing: typeof rotated) =>
        checkOutputKeys({
          outputs: [
            {
              ownerAddress: ShieldedAddress.fromPublicKeys(
                old.recipient.signingPublicKey,
                ZERO_NULLIFIER_PK as Bytes32,
                viewing,
              ),
            } as ProofOutputUtxo,
          ],
          rowOf: () => row,
          spenders: [],
        });
      expect(() => paying(old.recipient.viewingPublicKey)).toThrow(
        expect.objectContaining({ code: "RING_RECIPIENT_ENROLLED" }),
      );
      expect(() => paying(resolved.viewingPublicKey)).toThrow(
        expect.objectContaining({ code: "RING_RECIPIENT_ENROLLED" }),
      );
    });

    it("lets a merge preserve an enrolled owner's zero key", async () => {
      const { row } = await addressed();
      const client = chain(row, "present");
      const { root } = await client.getTrackingOpenings({
        registry: REGISTRY,
        members: [row.member],
        expectedRoot: ZERO_32,
      });
      const [opening] = await readOutputKeys({
        client,
        root: { ringProgramId: RING, registry: REGISTRY, revision: 1n, root },
        outputs: [{ member: row.member, nullifierPk: ZERO_NULLIFIER_PK as Bytes32 }],
      });
      expect(opening?.kind).toBe("present");
    });
  });

  it("refuses an opening under another root", async () => {
    const { to, row, seal } = await addressed();
    const client = chain(row, "present", seal);
    const other = chain({ ...row, ctHash: new Uint8Array(32).fill(9) as Bytes32 }, "present");
    await expect(
      resolveRingRecipient({
        client: { ...client, getTrackingOpenings: other.getTrackingOpenings },
        trackingRegistry: REGISTRY,
        to,
      }),
    ).rejects.toMatchObject({ code: "RING_TRACKING_INVALID" });
  });

  it("refuses any opening count but one for the one owner asked", async () => {
    const { to, row, seal } = await addressed();
    const client = chain(row, "present", seal);
    const served = await client.getTrackingOpenings({
      registry: REGISTRY,
      members: [],
      expectedRoot: new Uint8Array(32) as Bytes32,
    });
    for (const openings of [[], [...served.openings, ...served.openings]])
      await expect(
        resolveRingRecipient({
          client: { ...client, getTrackingOpenings: async () => ({ ...served, openings }) },
          trackingRegistry: REGISTRY,
          to,
        }),
      ).rejects.toMatchObject({
        code: "RING_TRACKING_INVALID",
        details: { reason: "openings", count: openings.length },
      });
  });
});
