import type { MessagePartialSigner } from "@solana/kit";
import type { Address, Bytes32, Bytes64 } from "@heliuslabs/spr/interface";
import { addressBytes, copyBytes, concatBytes } from "@heliuslabs/spr/internal";
import type { X25519PublicKey } from "@heliuslabs/spr/keypair";
import {
  encoder,
  base58Decoder,
  base64Decoder,
  requestAddress,
  checkedUnixTimestamp,
  exactRequestRecord,
  requestBytes,
} from "./wire.js";
import { AttestationRequest } from "./signed-request.js";
import { signMessage } from "../../protocol/ring-rpc/read-signer.js";

const AUDITOR_KEY_DOMAIN = "zolana/ring-auditor-key";
const AUDITOR_KEY_REQUEST_DOMAIN = "zolana/ring-rpc-auditor-key-request";

/** Mirrors Rust `AuditorKeyStatement::message`, bound to the cluster and the nonce of the request it answers. */
export function auditorKeyStatement(
  input: Readonly<{
    cluster: Bytes32;
    ringProgramId: Address;
    auditorPublicKey: X25519PublicKey;
    nonce: Bytes32;
  }>,
): Uint8Array {
  return concatBytes(
    encoder.encode(AUDITOR_KEY_DOMAIN),
    copyBytes(input.cluster, 32, "cluster"),
    addressBytes(input.ringProgramId, "ringProgramId"),
    input.auditorPublicKey.toBytes(),
    copyBytes(input.nonce, 32, "nonce"),
  );
}

/** Mirrors Rust `AuditorKeyAttestation::bytes`, the genesis hash pins the cluster. */
export function auditorKeyRequestAttestation(
  input: Readonly<{
    genesisHash: Bytes32;
    ringProgramId: Address;
    timestamp: bigint;
    nonce: Bytes32;
  }>,
): Uint8Array {
  return encoder.encode(
    [
      AUDITOR_KEY_REQUEST_DOMAIN,
      `genesis: ${base58Decoder.decode(input.genesisHash)}`,
      `ring: ${input.ringProgramId}`,
      `timestamp: ${input.timestamp}`,
      `nonce: ${base64Decoder.decode(input.nonce)}`,
    ].join("\n"),
  );
}

/** Mirrors Rust `CreateAuditorKeyRequest` after `AuditorKeyRequest::sign`. */
export interface SignedAuditorKeyRequest {
  readonly ringProgramId: Address;
  readonly authority: Address;
  readonly genesisHash: Bytes32;
  readonly timestamp: bigint;
  readonly nonce: Bytes32;
  readonly signature: Bytes64;
}

/** Mirrors Rust `AuditorKeyRequest`. */
export class RingAuditorKeyRequest extends AttestationRequest {
  readonly #ringProgramId: Address;
  readonly #genesisHash: Bytes32;

  private constructor(ringProgramId: Address, genesisHash: Bytes32) {
    super();
    this.#ringProgramId = ringProgramId;
    this.#genesisHash = genesisHash;
  }

  static forRing(ringProgramId: Address, genesisHash: Bytes32): RingAuditorKeyRequest {
    return new RingAuditorKeyRequest(
      requestAddress(ringProgramId, "ringProgramId"),
      copyBytes(genesisHash, 32, "genesisHash") as Bytes32,
    );
  }

  async sign(authority: MessagePartialSigner): Promise<SignedAuditorKeyRequest> {
    const { timestamp, nonce } = this.stamp();
    const signature = await signMessage(
      authority,
      auditorKeyRequestAttestation({
        genesisHash: this.#genesisHash,
        ringProgramId: this.#ringProgramId,
        timestamp,
        nonce,
      }),
    );
    return Object.freeze({
      ringProgramId: this.#ringProgramId,
      authority: authority.address,
      genesisHash: this.#genesisHash,
      timestamp,
      nonce,
      signature,
    });
  }
}

/** Mirrors Rust `CreateAuditorKeyResponse`, the signature covers `auditorKeyAttestation`. */
export interface RingAuditorKey {
  readonly ringProgramId: Address;
  readonly auditorPublicKey: X25519PublicKey;
  readonly auditorViewTag: Bytes32;
  readonly servicePublicKey: Address;
  readonly signature: Uint8Array;
}
const SIGNED_AUDITOR_FIELDS = new Set([
  "ringProgramId",
  "authority",
  "genesisHash",
  "timestamp",
  "nonce",
  "signature",
]);

export function checkedSignedAuditorKeyRequest(value: unknown): SignedAuditorKeyRequest {
  const request = exactRequestRecord(
    value,
    SIGNED_AUDITOR_FIELDS,
    SIGNED_AUDITOR_FIELDS,
    "request",
  );
  return Object.freeze({
    ringProgramId: requestAddress(request["ringProgramId"], "ringProgramId"),
    authority: requestAddress(request["authority"], "authority"),
    genesisHash: requestBytes(request["genesisHash"], 32, "genesisHash") as Bytes32,
    nonce: requestBytes(request["nonce"], 32, "nonce") as Bytes32,
    signature: requestBytes(request["signature"], 64, "signature") as Bytes64,
    timestamp: checkedUnixTimestamp(request["timestamp"], "timestamp"),
  });
}
