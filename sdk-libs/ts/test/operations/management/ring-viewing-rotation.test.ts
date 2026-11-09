import { address } from "@solana/kit";
import { beforeAll, describe, expect, it } from "vitest";

import { initializePoseidon } from "@heliuslabs/spr";
import type { Bytes16, Bytes32 } from "@heliuslabs/spr/interface";
import { ShieldedKeypair, SigningKey, ViewingKey } from "@heliuslabs/spr/keypair";

import { encodeEnrollmentRecord } from "../../../src/protocol/ring-rpc/relay-enroll-inputs.js";
import {
  confirmViewingKey,
  rotateViewingKey,
  updateViewingKey,
} from "../../../src/operations/management/viewing-rotation.js";
import { viewingKeyHash } from "../../../src/protocol/membership/tracking.js";
import { LocalRingKeys } from "../../../src/state/ring-keys.js";
import { ViewingHistory } from "../../../src/state/viewing-history.js";
import { enrolledEntry } from "../../helpers/member-registry.js";
import { ringTrackingClient } from "../../helpers/tracking-client.js";

const REGISTRY = address("9EwHno8C1T1vVGjasGnDH1GubiEu8qbgLX9qDjBshFhz");
const RING = address("9vyTbYGyh3cwxkAQpjjFQGXmdJP6p9B6YcQ5pNuXPNbh");
const SALT = new Uint8Array(16).fill(5) as Bytes16;

const wallet = (seed: number) =>
  ShieldedKeypair.fromKeypair(
    SigningKey.fromEd25519Bytes(new Uint8Array(32).fill(seed) as Bytes32),
  );

beforeAll(initializePoseidon);

/** `holder`'s leaf under the newest root when `applied`, `apply` moves its viewing key. */
async function enrolled(holder: ShieldedKeypair, applied = true) {
  const owner = holder.shieldedAddress();
  const entry = enrolledEntry(owner);
  const leaves = applied ? [entry] : [];
  // An apply of a present owner's entry moves only its viewing key.
  const apply = (viewingU: Bytes32) => {
    const seal = entry.seal;
    if (seal === undefined) throw new Error("no seal");
    leaves.splice(0, 1, {
      ...entry,
      viewingHash: viewingKeyHash(viewingU),
      seal: { ...seal, viewingU },
    });
  };
  return {
    owner: owner.solanaAddress(),
    client: await ringTrackingClient(RING, REGISTRY, leaves),
    apply,
  };
}

function rotation(holder: ShieldedKeypair, next: ViewingKey) {
  return rotateViewingKey({
    keys: LocalRingKeys.fromKeypair(holder),
    history: ViewingHistory.create(holder.viewingPublicKey(), 0n),
    next,
    rotatedAt: 1n,
  });
}

describe("viewing key rotation through the tracking row", () => {
  it("requests the new key without a proof and confirms it once an apply moves the leaf", async () => {
    const holder = wallet(1);
    const { owner, client, apply } = await enrolled(holder);
    const rotated = rotation(holder, ViewingKey.generate());
    const current = rotated.keys.address().viewingPublicKey.u();
    const update = () =>
      updateViewingKey({
        client,
        owner,
        ringProgramId: RING,
        keys: rotated.keys,
        history: rotated.history,
      });
    const confirm = () =>
      confirmViewingKey({ client, owner, trackingRegistry: REGISTRY, keys: rotated.keys });

    // 1. The rotation repeats the seal, carries the new key and no proof.
    const request = await update();
    if (request === undefined) throw new Error("a rotation");
    expect(request.trackingRegistry).toBe(REGISTRY);
    expect(request.proof).toBeUndefined();
    expect(encodeEnrollmentRecord(request.record).subarray(64, 96)).toEqual(current);
    await expect(confirm()).rejects.toMatchObject({ code: "RING_VIEWING_KEY_NOT_PUBLISHED" });

    // 2. A rotation Ring RPC holds confirms nothing until an apply moves the leaf.
    apply(current);
    expect((await confirm()).viewingU).toEqual(current);
    expect(await update()).toBeUndefined();
  });

  it("confirms nothing for a first enrollment no apply has inserted", async () => {
    const holder = wallet(5);
    const { owner, client } = await enrolled(holder, false);
    await expect(
      confirmViewingKey({
        client,
        owner,
        trackingRegistry: REGISTRY,
        keys: LocalRingKeys.fromKeypair(holder),
      }),
    ).rejects.toMatchObject({ code: "RING_VIEWING_KEY_NOT_PUBLISHED" });
  });

  it("opens a note sealed to the old key only with the retained key", async () => {
    const holder = wallet(2);
    const next = ViewingKey.generate();
    const rotated = rotation(holder, next);
    const tx = ViewingKey.generate();
    const old = holder.viewingPublicKey();
    const plaintext = new TextEncoder().encode("old note");
    const request = {
      ciphertext: tx.encryptSlot(old, plaintext, SALT, 0),
      viewingPublicKey: old,
      txViewingPublicKey: tx.publicKey(),
      salt: SALT,
      slotIndex: 0,
      label: "transfer",
    } as const;
    expect(await rotated.keys.decrypt([request])).toEqual([plaintext]);

    const currentOnly = LocalRingKeys.fromKeys({
      address: rotated.keys.address(),
      viewingKeys: [next],
      listRoot: holder.viewingKey(),
      nullifierKey: holder.nullifierKey(),
    });
    await expect(currentOnly.decrypt([request])).rejects.toThrow();
  });

  it("refuses a holder of another nullifier key and one ahead of its history", async () => {
    const holder = wallet(3);
    const { owner, client } = await enrolled(holder);
    const stranger = rotation(wallet(4), ViewingKey.generate());
    const input = { client, owner, ringProgramId: RING };
    await expect(
      updateViewingKey({ ...input, keys: stranger.keys, history: stranger.history }),
    ).rejects.toMatchObject({ code: "RING_ENROLLMENT_KEY_MISMATCH" });
    await expect(
      updateViewingKey({
        ...input,
        keys: LocalRingKeys.fromKeypair(holder).rotate(ViewingKey.generate()),
        history: ViewingHistory.create(holder.viewingPublicKey(), 0n),
      }),
    ).rejects.toMatchObject({ code: "RING_VIEWING_HISTORY_NOT_CURRENT" });
  });
});
