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
  checkEnrollment,
  enrollmentRequest,
  enrollmentState,
  ownerGrantInstruction,
  proveEnrollment,
  type EnrollmentSeal,
  type EnrollmentState,
} from "../../../src/operations/management/enrollment.js";
import {
  encodeEnrollmentWrite,
  encodeRelayEnrollInputs,
  signedWrite,
  enrollmentWriteEntry,
} from "../../../src/protocol/ring-rpc/relay-enroll-inputs.js";
import { QUAD_CAPACITY, QUAD_HEIGHT } from "../../../src/protocol/membership/member-registry.js";
import { memberOfTag } from "../../../src/protocol/membership/policy.js";
import {
  TRACKING_EMPTY_ROOT,
  TRACKING_ROOT_HISTORY,
  TRACKING_SENTINEL,
  decodeOwnerGrant,
  decodeTrackingRegistry,
  trackingEntryHash,
  trackingEntryOf,
  trackingRecordHash,
  verifyTrackingOpening,
  viewingKeyHash,
  type TrackingRecord,
} from "../../../src/protocol/membership/tracking.js";
import { ownerGrantPda } from "../../../src/protocol/pda/index.js";
import { registerKeyStatementHash } from "../../../src/protocol/proof/statement.js";
import type { RegisterKeyProofRequest } from "../../../src/rpc/prover/ring.js";
import { poseidon } from "@heliuslabs/spr/internal";
import type { FixtureTrackingEntry } from "../../helpers/member-registry.js";
import { ringTrackingClient, trackingClient } from "../../helpers/tracking-client.js";

const REGISTRY = address("Stor111111111111111111111111111111111111111");
const OWNER = address("Cura111111111111111111111111111111111111111");
const RING = address("9vyTbYGyh3cwxkAQpjjFQGXmdJP6p9B6YcQ5pNuXPNbh");
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
  return { root: field(0x01), record: TRACKING_SENTINEL, seal: SEAL, viewingU: VIEWING_U };
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

/** The ring pinning `REGISTRY`, its tree holding `applied`. */
function ringClient(applied: readonly FixtureTrackingEntry[] = []) {
  return ringTrackingClient(RING, REGISTRY, [...applied]);
}

beforeAll(initializePoseidon);

describe("checkEnrollment", () => {
  it("passes a first enrollment and refuses an owner a root already holds", () => {
    expect(() => checkEnrollment({ seal: SEAL, stored: undefined })).not.toThrow();
    expect(() => checkEnrollment({ seal: SEAL, stored: stored() })).toThrow(
      "RING_ALREADY_ENROLLED",
    );
  });

  it("refuses the zero key before the owner signs", () => {
    expect(() =>
      checkEnrollment({
        seal: { ...SEAL, nullifierPk: ZERO_NULLIFIER_PK as Bytes32 },
        stored: undefined,
      }),
    ).toThrow("RING_ENROLLMENT_ZERO_KEY");
  });
});

describe("tracking accounts", () => {
  it("decodes a registry of its authority and 128 roots and refuses a next index past the tree", () => {
    const data = new Uint8Array(106 + 48 * TRACKING_ROOT_HISTORY);
    expect(data).toHaveLength(6250);
    data[0] = 3;
    data.set(addressBytes(OWNER, "authority"), 1);
    data.set(field(0x21), 33);
    data[65] = 4;
    const nextIndex = (value: bigint) => new DataView(data.buffer).setBigUint64(98, value, true);
    nextIndex(1n);
    data.set(TRACKING_EMPTY_ROOT, 106);
    data.set(field(0x22), 106 + 48 * (TRACKING_ROOT_HISTORY - 1));
    const decoded = decodeTrackingRegistry(data);
    expect(decoded.authority).toBe(OWNER);
    expect(decoded.ringId).toEqual(field(0x21));
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
    expect(() => decodeTrackingRegistry(data.subarray(0, 6218))).toThrow("RING_TRACKING_INVALID");
  });

  it("binds the proven bit and the viewing key in the entry hash", () => {
    const entry = trackingEntryOf({
      member: field(0x05),
      nullifierPk: SEAL.nullifierPk,
      listTag: SEAL.listTag,
      viewingU: VIEWING_U,
      ciphertext: SEAL.ciphertext,
    });
    expect(entry.proven).toBe(true);
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

describe("enrollment requests", () => {
  it("lays out an Enroll write as owner, kind 0, record and proof under the ring's registry", async () => {
    const first = await enrollmentRequest(
      { ringProgramId: RING, owner: OWNER, seal: SEAL, viewingU: VIEWING_U, proof: PROOF },
      await ringClient(),
    );
    expect(first).toMatchObject({ ringProgramId: RING, trackingRegistry: REGISTRY });
    const write = encodeEnrollmentWrite(first.write);
    expect(write).toHaveLength(417);
    const data = new Reader(write);
    expect(data.bytes(32, "owner")).toEqual(addressBytes(OWNER, "owner"));
    expect(data.u8("kind")).toBe(0);
    expect(data.bytes(32, "nullifierPk")).toEqual(SEAL.nullifierPk);
    expect(data.bytes(32, "listTag")).toEqual(SEAL.listTag);
    expect(data.bytes(32, "viewingU")).toEqual(VIEWING_U);
    expect(data.bytes(32, "ephU")).toEqual(SEAL.ephU);
    expect(data.bytes(64, "ciphertext")).toEqual(SEAL.ciphertext);
    expect(data.bytes(192, "proof")).toEqual(PROOF);
    data.done();
    expect(encodeRelayEnrollInputs(first).subarray(64)).toEqual(write);
  });

  it("signs a 105-byte message over the registry, the owner and the proven entry hash", () => {
    const enroll = {
      owner: OWNER,
      change: { kind: "enroll", record: { ...SEAL, viewingU: VIEWING_U }, proof: PROOF },
    } as const;
    const entry = enrollmentWriteEntry(enroll);
    expect(entry).toMatchObject({ proven: true, ctHash: hashBytes(SEAL.ciphertext) });
    const signed = signedWrite(enroll, REGISTRY);
    expect(signed.entryHash).toEqual(trackingEntryHash(entry));
    const { message } = signed;
    expect(message).toHaveLength(105);
    expect(new TextDecoder().decode(message.subarray(0, 9))).toBe("CRING/enr");
    expect(message.subarray(9, 41)).toEqual(addressBytes(REGISTRY, "registry"));
    expect(message.subarray(41, 73)).toEqual(addressBytes(OWNER, "owner"));
    expect(message.subarray(73)).toEqual(trackingEntryHash(entry));
    // A write signed for one registry applies to no other.
    expect(signedWrite(enroll, RING).message).not.toEqual(message);
    expect(entry.priorViewingHash).toEqual(ZERO_32);
    const prior = hashBytes(field(0x16)) as Bytes32;
    const rotate = {
      owner: OWNER,
      change: { kind: "rotate", viewingU: VIEWING_U, priorViewingHash: prior },
    } as const;
    expect(encodeEnrollmentWrite(rotate)).toHaveLength(97);
    expect(encodeEnrollmentWrite(rotate).subarray(65)).toEqual(prior);
    expect(enrollmentWriteEntry(rotate)).toMatchObject({
      nullifierPk: ZERO_32,
      ctHash: ZERO_32,
      listTag: ZERO_32,
      viewingHash: hashBytes(VIEWING_U),
      priorViewingHash: prior,
      proven: false,
    });
    expect(signedWrite(rotate, REGISTRY).message).not.toEqual(message);
    // A rotation signed against one prior key applies to no other.
    const other = { ...rotate, change: { ...rotate.change, priorViewingHash: ZERO_32 } };
    expect(signedWrite(other, REGISTRY).message).not.toEqual(signedWrite(rotate, REGISTRY).message);
  });

  it("checks the owner's leaf under the newest root before the owner signs", async () => {
    const request = async (applied: readonly FixtureTrackingEntry[]) =>
      enrollmentRequest(
        { ringProgramId: RING, owner: OWNER, seal: SEAL, viewingU: field(0x16), proof: PROOF },
        await ringClient(applied),
      );
    await expect(request([])).resolves.toBeDefined();
    await expect(request([sealedEntry(OWNER)])).rejects.toMatchObject({
      code: "RING_ALREADY_ENROLLED",
    });
    await expect(
      enrollmentRequest(
        {
          ringProgramId: RING,
          owner: OWNER,
          seal: { ...SEAL, nullifierPk: ZERO_NULLIFIER_PK as Bytes32 },
          viewingU: VIEWING_U,
          proof: PROOF,
        },
        await ringClient(),
      ),
    ).rejects.toMatchObject({ code: "RING_ENROLLMENT_ZERO_KEY" });
  });

  it("refuses a noncanonical viewing key and a proof off its length", async () => {
    const client = await ringClient();
    await expect(
      enrollmentRequest(
        {
          ringProgramId: RING,
          owner: OWNER,
          seal: SEAL,
          viewingU: new Uint8Array(32).fill(0xff) as Bytes32,
          proof: PROOF,
        },
        client,
      ),
    ).rejects.toMatchObject({ code: "RING_NONCANONICAL_VIEWING_KEY" });
    await expect(
      enrollmentRequest(
        {
          ringProgramId: RING,
          owner: OWNER,
          seal: SEAL,
          viewingU: VIEWING_U,
          proof: PROOF.subarray(1),
        },
        client,
      ),
    ).rejects.toMatchObject({ code: "RING_PROOF_LENGTH" });
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
    const data = new Uint8Array(6250);
    data[0] = 3;
    data.set(ringId, 33);
    data.set(auditor.publicKey().toBytes(), 65);
    new DataView(data.buffer).setBigUint64(98, 1n, true);
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

  it("reads the applied leaf and nothing for an owner no root holds", async () => {
    const applied = await enrollmentState(
      { trackingRegistry: REGISTRY, owner: OWNER },
      trackingClient(REGISTRY, [sealedEntry(OWNER)]),
    );
    expect(applied).toMatchObject({ seal: SEAL, viewingU: VIEWING_U });
    expect(
      await enrollmentState(
        { trackingRegistry: REGISTRY, owner: OWNER },
        trackingClient(REGISTRY, []),
      ),
    ).toBeUndefined();
  });

  it("refuses a served viewing key the leaf does not hash", async () => {
    const entry = sealedEntry(OWNER);
    const forged = { ...entry, seal: { ...entry.seal!, viewingU: field(0x17) } };
    await expect(
      enrollmentState(
        { trackingRegistry: REGISTRY, owner: OWNER },
        trackingClient(REGISTRY, [forged]),
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
