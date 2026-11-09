import { address, generateKeyPairSigner, type Address } from "@solana/kit";
import { beforeAll, describe, expect, it, vi } from "vitest";

import { readFileSync } from "node:fs";

import { initializePoseidon } from "@heliuslabs/spr";
import {
  Reader,
  ZERO_32,
  ZERO_NULLIFIER_PK,
  addressBytes,
  hashBytes,
} from "@heliuslabs/spr/internal";
import type { Bytes32, Bytes33, Bytes64 } from "@heliuslabs/spr/interface";
import { ShieldedKeypair, SigningKey, X25519Keypair } from "@heliuslabs/spr/keypair";
import { hexToBytes } from "@noble/hashes/utils.js";

import { listTagOf } from "../../../src/crypto/keys/list-salt.js";
import {
  applyEnrollmentsInstruction,
  checkEnrollment,
  enrollInstruction,
  enrollmentState,
  ownerGrantInstruction,
  proveEnrollment,
  type EnrollmentReadClient,
  type EnrollmentSeal,
  type EnrollmentState,
} from "../../../src/operations/management/enrollment.js";
import type { QueuedEnrollment } from "../../../src/protocol/indexer/tracking-codec.js";
import { QUAD_CAPACITY, QUAD_HEIGHT } from "../../../src/protocol/membership/member-registry.js";
import { memberOfTag } from "../../../src/protocol/membership/policy.js";
import {
  ENROLLMENT_QUEUE_CAPACITY,
  TRACKING_EMPTY_ROOT,
  TRACKING_ROOT_HISTORY,
  TRACKING_SENTINEL,
  checkEnrollmentQueueOpen,
  decodeEnrollmentQueue,
  decodeOwnerGrant,
  decodeTrackingRegistry,
  queuedEntryHashes,
  trackingEntryHash,
  trackingEntryOf,
  trackingRecordHash,
  verifyTrackingOpening,
  viewingKeyHash,
  type TrackingRecord,
} from "../../../src/protocol/membership/tracking.js";
import {
  enrollmentQueuePda,
  ownerGrantPda,
  STORAGE_PROGRAM_ID,
} from "../../../src/protocol/pda/index.js";
import { registerKeyStatementHash } from "../../../src/protocol/proof/statement.js";
import type { RegisterKeyProofRequest } from "../../../src/rpc/prover/ring.js";
import { poseidon } from "@heliuslabs/spr/internal";
import type { FixtureTrackingEntry } from "../../helpers/member-registry.js";
import { trackingClient } from "../../helpers/tracking-client.js";

const REGISTRY = address("Stor111111111111111111111111111111111111111");
const OWNER = address("Cura111111111111111111111111111111111111111");
const field = (value: number): Bytes32 => new Uint8Array(32).fill(value) as Bytes32;
const PROOF = new Uint8Array(192).fill(5);
const VIEWING_U = field(0x15);

const SEAL: EnrollmentSeal = Object.freeze({
  nullifierPk: field(0x11),
  listTag: field(0x12),
  ephU: field(0x13),
  ciphertext: new Uint8Array(64).fill(0x14) as Bytes64,
});

function stored(): EnrollmentState {
  return { kind: "queued", sequence: 0n, seal: SEAL, viewingU: VIEWING_U };
}

/** `owner`'s leaf as an applied `SEAL` enrollment would insert it. */
function sealedEntry(owner: Address, viewingU: Bytes32 = VIEWING_U): FixtureTrackingEntry {
  return {
    member: memberOfTag(addressBytes(owner, "owner")),
    nullifierPk: SEAL.nullifierPk,
    ctHash: hashBytes(SEAL.ciphertext) as Bytes32,
    listTag: SEAL.listTag,
    viewingHash: viewingKeyHash(viewingU),
    seal: { owner, viewingU, ephPk: Uint8Array.of(4, ...SEAL.ephU), ciphertext: SEAL.ciphertext },
  };
}

function queued(owner: Address, sequence: bigint, proven: boolean): QueuedEnrollment {
  return {
    sequence,
    owner,
    member: memberOfTag(addressBytes(owner, "owner")),
    proven,
    nullifierPk: SEAL.nullifierPk,
    listTag: SEAL.listTag,
    viewingU: VIEWING_U,
    ephPk: Uint8Array.of(4, ...SEAL.ephU),
    ciphertext: SEAL.ciphertext,
  };
}

function enrollmentClient(
  input: Readonly<{
    applied?: readonly FixtureTrackingEntry[];
    pending?: readonly QueuedEnrollment[];
    queued?: number;
  }> = {},
): EnrollmentReadClient {
  return trackingClient(REGISTRY, {
    applied: [...(input.applied ?? [])],
    pending: [...(input.pending ?? [])],
    ...(input.queued === undefined ? {} : { queued: input.queued }),
  });
}

function queueBytes(head: bigint, tail: bigint, registry: Address = REGISTRY): Uint8Array {
  const data = new Uint8Array(49 + 32 * ENROLLMENT_QUEUE_CAPACITY);
  data[0] = 4;
  data.set(addressBytes(registry, "registry"), 1);
  const view = new DataView(data.buffer);
  view.setBigUint64(33, head, true);
  view.setBigUint64(41, tail, true);
  for (let sequence = head; sequence < tail; sequence += 1n)
    data.set(field(Number(sequence % 256n)), 49 + 32 * Number(sequence % 256n));
  return data;
}

beforeAll(initializePoseidon);

describe("checkEnrollment", () => {
  it("passes a first enrollment and a rotation of the viewing key", () => {
    expect(() => checkEnrollment({ seal: SEAL, stored: undefined })).not.toThrow();
    expect(() => checkEnrollment({ seal: SEAL, stored: stored() })).not.toThrow();
  });

  it("refuses the zero key before the owner signs", () => {
    expect(() =>
      checkEnrollment({
        seal: { ...SEAL, nullifierPk: ZERO_NULLIFIER_PK as Bytes32 },
        stored: undefined,
      }),
    ).toThrow("RING_ENROLLMENT_ZERO_KEY");
  });

  it.each(["nullifierPk", "listTag", "ephU", "ciphertext"] as const)(
    "refuses a changed %s, an entry of a present owner moves only its viewing key",
    (name) => {
      expect(() =>
        checkEnrollment({
          seal: { ...SEAL, [name]: name === "ciphertext" ? new Uint8Array(64) : field(0x77) },
          stored: stored(),
        }),
      ).toThrow("RING_ENROLLMENT_KEY_MISMATCH");
    },
  );
});

describe("tracking accounts", () => {
  it("decodes a registry of 128 roots and refuses a next index past the tree", () => {
    const data = new Uint8Array(74 + 48 * TRACKING_ROOT_HISTORY);
    expect(data).toHaveLength(6218);
    data[0] = 3;
    data.set(field(0x21), 1);
    data[33] = 4;
    const nextIndex = (value: bigint) => new DataView(data.buffer).setBigUint64(66, value, true);
    nextIndex(1n);
    data.set(TRACKING_EMPTY_ROOT, 74);
    data.set(field(0x22), 74 + 48 * (TRACKING_ROOT_HISTORY - 1));
    const decoded = decodeTrackingRegistry(data);
    expect(decoded.nextIndex).toBe(1n);
    expect(decoded.roots).toHaveLength(TRACKING_ROOT_HISTORY);
    expect(decoded.roots[0]?.hash).toEqual(TRACKING_EMPTY_ROOT);
    expect(decoded.roots[1]).toBeUndefined();
    expect(decoded.roots[TRACKING_ROOT_HISTORY - 1]?.hash).toEqual(field(0x22));
    // A full tree's next index is its capacity, the u64 field holds no index past it.
    nextIndex(QUAD_CAPACITY);
    expect(decodeTrackingRegistry(data).nextIndex).toBe(1n << 32n);
    nextIndex(QUAD_CAPACITY + 1n);
    expect(() => decodeTrackingRegistry(data)).toThrow("RING_TRACKING_INVALID");
    nextIndex(0n);
    expect(() => decodeTrackingRegistry(data)).toThrow("RING_TRACKING_INVALID");
    expect(() => decodeTrackingRegistry(data.subarray(0, 1610))).toThrow("RING_TRACKING_INVALID");
  });

  it("decodes a queue, sequence s at s % 256, and takes the entries from head", () => {
    const queue = decodeEnrollmentQueue(queueBytes(250n, 260n));
    expect(queue).toMatchObject({ registry: REGISTRY, head: 250n, tail: 260n });
    expect(queuedEntryHashes(queue, 10)).toEqual(
      [250, 251, 252, 253, 254, 255, 0, 1, 2, 3].map(field),
    );
    expect(() => queuedEntryHashes(queue, 11)).toThrow("RING_ENROLLMENT_QUEUE_SHORT");
    expect(() => queuedEntryHashes(queue, 0)).toThrow("RING_ENROLLMENT_NONE_NAMED");
    expect(() => checkEnrollmentQueueOpen(queue)).not.toThrow();
  });

  it("refuses a full queue push and a queue off its layout", () => {
    const full = decodeEnrollmentQueue(queueBytes(3n, 3n + 256n));
    expect(() => checkEnrollmentQueueOpen(full)).toThrow("RING_ENROLLMENT_QUEUE_FULL");
    expect(() => decodeEnrollmentQueue(queueBytes(3n, 3n + 257n))).toThrow("RING_TRACKING_INVALID");
    expect(() => decodeEnrollmentQueue(queueBytes(5n, 4n))).toThrow("RING_TRACKING_INVALID");
    expect(() => decodeEnrollmentQueue(queueBytes(0n, 0n).subarray(1))).toThrow(
      "RING_TRACKING_INVALID",
    );
    const other = queueBytes(0n, 0n);
    other[0] = 3;
    expect(() => decodeEnrollmentQueue(other)).toThrow("RING_TRACKING_INVALID");
  });

  it("binds the proven bit and the viewing key in the entry hash", () => {
    const entry = trackingEntryOf({
      member: field(0x05),
      nullifierPk: SEAL.nullifierPk,
      listTag: SEAL.listTag,
      viewingU: VIEWING_U,
      ciphertext: SEAL.ciphertext,
      proven: true,
    });
    expect(entry.ctHash).toEqual(hashBytes(SEAL.ciphertext));
    expect(entry.viewingHash).toEqual(hashBytes(VIEWING_U));
    expect(trackingEntryHash({ ...entry, proven: false })).not.toEqual(trackingEntryHash(entry));
    expect(trackingEntryHash({ ...entry, viewingHash: field(0x01) })).not.toEqual(
      trackingEntryHash(entry),
    );
  });

  it("decodes an owner grant, only one means active", () => {
    const data = new Uint8Array(2);
    data[0] = 5;
    data[1] = 1;
    expect(decodeOwnerGrant(data).active).toBe(true);
    data[1] = 0;
    expect(decodeOwnerGrant(data).active).toBe(false);
  });
});

describe("tracking openings", () => {
  const paths = () => {
    const zeros: Bytes32[] = [ZERO_32];
    for (let level = 0; level < QUAD_HEIGHT; level += 1) {
      const below = zeros[level] as Bytes32;
      zeros.push(poseidon([below, below, below, below]) as Bytes32);
    }
    return zeros;
  };

  /** The tree holding only `leaf` at index 0. */
  function single(leaf: TrackingRecord) {
    const zeros = paths();
    const siblings = zeros.slice(0, QUAD_HEIGHT).map((zero) => [zero, zero, zero]);
    let root = trackingRecordHash(leaf);
    for (const level of siblings) root = poseidon([root, ...level]) as Bytes32;
    return { root, path: { index: 0, siblings } };
  }

  it("folds the sentinel to the registry's first root", () => {
    expect(single(TRACKING_SENTINEL).root).toEqual(TRACKING_EMPTY_ROOT);
  });

  it("proves absence through the low row and refuses a member outside it", () => {
    const { root, path } = single(TRACKING_SENTINEL);
    expect(
      verifyTrackingOpening({ kind: "absent", low: TRACKING_SENTINEL, path }, root, field(0x05)),
    ).toBeUndefined();
    const member = field(0x05);
    const low = { ...TRACKING_SENTINEL, next: field(0x04) };
    const short = single(low);
    expect(() =>
      verifyTrackingOpening({ kind: "absent", low, path: short.path }, short.root, member),
    ).toThrow("RING_TRACKING_INVALID");
  });

  it("returns a present row only under its own root and member", () => {
    const record = { ...TRACKING_SENTINEL, member: field(0x05), revision: 2n };
    const { root, path } = single(record);
    expect(verifyTrackingOpening({ kind: "present", record, path }, root, field(0x05))).toEqual(
      record,
    );
    expect(() =>
      verifyTrackingOpening({ kind: "present", record, path }, field(0x06), field(0x05)),
    ).toThrow("RING_TRACKING_INVALID");
    expect(() =>
      verifyTrackingOpening({ kind: "present", record, path }, root, field(0x06)),
    ).toThrow("RING_TRACKING_INVALID");
  });
});

describe("enrollment instructions", () => {
  it("lays out tag 6, the fields then the proof on a first enrollment, none on a rotation", async () => {
    const owner = await generateKeyPairSigner();
    const client = enrollmentClient();
    const first = await enrollInstruction(
      { owner, trackingRegistry: REGISTRY, seal: SEAL, viewingU: VIEWING_U, proof: PROOF },
      client,
    );
    const data = new Reader(new Uint8Array(first.data ?? []));
    expect(data.u8("tag")).toBe(6);
    expect(data.bytes(32, "nullifierPk")).toEqual(SEAL.nullifierPk);
    expect(data.bytes(32, "listTag")).toEqual(SEAL.listTag);
    expect(data.bytes(32, "viewingU")).toEqual(VIEWING_U);
    expect(data.bytes(32, "ephU")).toEqual(SEAL.ephU);
    expect(data.bytes(64, "ciphertext")).toEqual(SEAL.ciphertext);
    expect(data.bytes(192, "proof")).toEqual(PROOF);
    data.done();
    expect(first.programAddress).toBe(STORAGE_PROGRAM_ID);
    // No payer and no system program, an Enroll moves no lamports and writes the queue alone.
    const [queue] = await enrollmentQueuePda(REGISTRY);
    expect(first.accounts?.map(({ address: key, role }) => [key, role])).toEqual([
      [owner.address, 2],
      [REGISTRY, 0],
      [queue, 1],
    ]);
    const rotation = await enrollInstruction(
      { owner, trackingRegistry: REGISTRY, seal: SEAL, viewingU: VIEWING_U },
      client,
    );
    expect(rotation.data).toHaveLength(1 + 192);
  });

  it("checks the owner's leaf under the newest root before it builds", async () => {
    const owner = await generateKeyPairSigner();
    const client = enrollmentClient({ applied: [sealedEntry(owner.address)] });
    const enroll = (seal: EnrollmentSeal) =>
      enrollInstruction({ owner, trackingRegistry: REGISTRY, seal, viewingU: field(0x16) }, client);
    await expect(enroll(SEAL)).resolves.toBeDefined();
    await expect(
      enroll({ ...SEAL, ciphertext: new Uint8Array(64).fill(0x16) as Bytes64 }),
    ).rejects.toMatchObject({ code: "RING_ENROLLMENT_KEY_MISMATCH" });
    await expect(
      enrollInstruction(
        {
          owner,
          trackingRegistry: REGISTRY,
          seal: { ...SEAL, nullifierPk: ZERO_NULLIFIER_PK as Bytes32 },
          viewingU: VIEWING_U,
        },
        enrollmentClient(),
      ),
    ).rejects.toMatchObject({ code: "RING_ENROLLMENT_ZERO_KEY" });
  });

  it("refuses an enrollment the full queue cannot take", async () => {
    const owner = await generateKeyPairSigner();
    await expect(
      enrollInstruction(
        { owner, trackingRegistry: REGISTRY, seal: SEAL, viewingU: VIEWING_U, proof: PROOF },
        enrollmentClient({ queued: ENROLLMENT_QUEUE_CAPACITY }),
      ),
    ).rejects.toMatchObject({ code: "RING_ENROLLMENT_QUEUE_FULL" });
  });

  it("names the registry and its queue, the count last, and refuses a count no key serves", async () => {
    const apply = (count: number) =>
      applyEnrollmentsInstruction({
        trackingRegistry: REGISTRY,
        revision: 9n,
        proof: PROOF,
        newRoot: field(0x31),
        newNextIndex: 3n,
        count,
      });
    const applied = await apply(3);
    expect(applied.data).toHaveLength(1 + 8 + 192 + 32 + 8 + 1);
    expect(applied.data?.subarray(1 + 8 + 192 + 32)).toEqual(
      Uint8Array.of(3, 0, 0, 0, 0, 0, 0, 0, 3),
    );
    const [queue] = await enrollmentQueuePda(REGISTRY);
    expect(applied.accounts?.map(({ address: key, role }) => [key, role])).toEqual([
      [REGISTRY, 1],
      [queue, 1],
    ]);
    await expect(apply(0)).rejects.toMatchObject({ code: "RING_ENROLLMENT_NONE_NAMED" });
    await expect(apply(17)).rejects.toMatchObject({
      code: "RING_STORAGE_WRITE_INVALID",
      details: { reason: "UnsupportedBatch" },
    });
  });

  it("writes an owner grant under the split reader key", async () => {
    const owner = await generateKeyPairSigner();
    const reader = address("9vyTbYGyh3cwxkAQpjjFQGXmdJP6p9B6YcQ5pNuXPNbh");
    const built = await ownerGrantInstruction({
      payer: owner,
      owner,
      trackingRegistry: REGISTRY,
      reader,
      active: true,
    });
    const data = new Uint8Array(built.data ?? []);
    expect(data[0]).toBe(8);
    expect(data).toHaveLength(1 + 34 + 1);
    expect(data[35]).toBe(1);
    const [grant] = await ownerGrantPda(REGISTRY, owner.address, data.subarray(1, 35));
    expect(built.accounts?.[3]?.address).toBe(grant);
  });
});

const SALT = Uint8Array.of(0, ...new Uint8Array(31).fill(9)) as Bytes32;

describe("proveEnrollment", () => {
  const auditor = X25519Keypair.generate();

  function registryAccount(ringId: Bytes32): { data: Uint8Array } {
    const data = new Uint8Array(6218);
    data[0] = 3;
    data.set(ringId, 1);
    data.set(auditor.publicKey().toBytes(), 33);
    new DataView(data.buffer).setBigUint64(66, 1n, true);
    return { data };
  }

  it("seals the ring key to the registry's auditor and states ring, member and list tag", async () => {
    const holder = ShieldedKeypair.fromKeypair(
      SigningKey.fromEd25519Bytes(new Uint8Array(32).fill(7) as Bytes32),
    );
    const ringId = field(0x1e);
    const seen: RegisterKeyProofRequest[] = [];
    const { seal, proof } = await proveEnrollment({
      holder,
      owner: OWNER,
      trackingRegistry: REGISTRY,
      listSalt: SALT,
      client: {
        getAccount: vi.fn(async () => registryAccount(ringId)),
      } as never,
      prover: {
        proveRing: async (request: RegisterKeyProofRequest) => {
          seen.push({ ...request });
          return PROOF;
        },
      } as never,
    });
    expect(proof).toEqual(PROOF);
    expect(seal.nullifierPk).toEqual(holder.nullifierPublicKey());
    expect(seal.listTag).toEqual(listTagOf(SALT));
    const [request] = seen;
    if (request === undefined) throw new Error("no proof asked");
    expect(request.ringId).toEqual(ringId);
    expect(request.member).toEqual(memberOfTag(addressBytes(OWNER, "owner")));
    expect(request.publicInputHash).toEqual(
      registerKeyStatementHash({
        ringId,
        member: request.member,
        nullifierPk: seal.nullifierPk,
        listTag: seal.listTag,
        auditorPubkey: auditor.publicKey().toBytes() as Bytes33,
        ephU: seal.ephU,
        ciphertext: seal.ciphertext,
      }),
    );
    // The holder wipes its copies once the prover answers.
    expect(request.nullifierSecret.every((byte) => byte === 0)).toBe(true);
    expect(request.ephSk.every((byte) => byte === 0)).toBe(true);
  });

  it("refuses a registry account it cannot read before any proof", async () => {
    const holder = ShieldedKeypair.fromKeypair(
      SigningKey.fromEd25519Bytes(new Uint8Array(32).fill(7) as Bytes32),
    );
    const proveRing = vi.fn();
    await expect(
      proveEnrollment({
        holder,
        owner: OWNER,
        trackingRegistry: REGISTRY,
        listSalt: SALT,
        client: { getAccount: async () => undefined } as never,
        prover: { proveRing } as never,
      }),
    ).rejects.toMatchObject({ code: "RING_TRACKING_INVALID" });
    expect(proveRing).not.toHaveBeenCalled();
  });

  it("reads the applied leaf, else the first proven queued entry, a rotation alone nothing", async () => {
    const applied = await enrollmentState(
      { trackingRegistry: REGISTRY, owner: OWNER },
      enrollmentClient({ applied: [sealedEntry(OWNER)] }),
    );
    expect(applied).toMatchObject({ kind: "applied", seal: SEAL, viewingU: VIEWING_U });
    const pending = [queued(OWNER, 0n, false), queued(REGISTRY, 1n, true), queued(OWNER, 2n, true)];
    expect(
      await enrollmentState(
        { trackingRegistry: REGISTRY, owner: OWNER },
        enrollmentClient({ pending }),
      ),
    ).toMatchObject({ kind: "queued", sequence: 2n, seal: SEAL });
    expect(
      await enrollmentState(
        { trackingRegistry: REGISTRY, owner: OWNER },
        enrollmentClient({ pending: [queued(OWNER, 0n, false)] }),
      ),
    ).toBeUndefined();
  });

  it("refuses a served viewing key the leaf does not hash", async () => {
    const entry = sealedEntry(OWNER);
    const forged = { ...entry, seal: { ...entry.seal!, viewingU: field(0x17) } };
    await expect(
      enrollmentState(
        { trackingRegistry: REGISTRY, owner: OWNER },
        enrollmentClient({ applied: [forged] }),
      ),
    ).rejects.toMatchObject({ code: "RING_VIEWING_KEY_MISMATCH" });
  });
});

describe("list salt vector", () => {
  const vector = JSON.parse(
    readFileSync(
      new URL("../../../../../test-vectors/proof/ring-statements.json", import.meta.url),
      "utf8",
    ),
  ) as { tracking: Record<string, string> };

  it("derives the list salt and tag the Go circuit proves", () => {
    const { list_salt: salt, secret_list_tag: tag } = vector.tracking;
    if (salt === undefined || tag === undefined) throw new Error("no vector");
    expect(Buffer.from(listTagOf(hexToBytes(salt) as Bytes32)).toString("hex")).toBe(tag);
  });
});
