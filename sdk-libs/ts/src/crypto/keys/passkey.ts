import { equalBytes, sha256 } from "@heliuslabs/spr/internal";

import { RingError } from "../../protocol/error.js";
import {
  P256ReaderKey,
  checkedReaderKey,
  readerKeyBytes,
} from "../../protocol/membership/reader-access.js";
import type { RingReadSigner, WebAuthnSignature } from "../../protocol/ring-rpc/read-signer.js";

export interface Passkey {
  readonly credentialId: Uint8Array;
  readonly publicKey: P256ReaderKey;
}

const ES256 = -7;
/** SPKI DER of a P-256 key ends with the 65-byte uncompressed point. */
const UNCOMPRESSED_POINT_LENGTH = 65;
const P256_SPKI_PREFIX = Uint8Array.from([
  0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a,
  0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
]);

/** Only ES256 is offered, the authenticator always answers with a P-256 key. */
export async function createPasskey(
  input: Readonly<{ rpName: string; userName: string }>,
): Promise<Passkey> {
  const credentials = webAuthn();
  const userId = new Uint8Array(16);
  crypto.getRandomValues(userId);
  const credential = (await credentials.create({
    publicKey: {
      rp: { name: input.rpName },
      user: { id: userId, name: input.userName, displayName: input.userName },
      challenge: crypto.getRandomValues(new Uint8Array(32)),
      pubKeyCredParams: [{ type: "public-key", alg: ES256 }],
      authenticatorSelection: { residentKey: "preferred", userVerification: "required" },
    },
  })) as PublicKeyCredential | null;
  const response = credential?.response as AuthenticatorAttestationResponse | undefined;
  const spki = response?.getPublicKey?.();
  if (!credential || !spki || response?.getPublicKeyAlgorithm?.() !== ES256) {
    throw new RingError("RING_PASSKEY", { details: { reason: "no ES256 credential" } });
  }
  const spkiBytes = new Uint8Array(spki);
  if (
    spkiBytes.length !== P256_SPKI_PREFIX.length + UNCOMPRESSED_POINT_LENGTH ||
    !equalBytes(spkiBytes.subarray(0, P256_SPKI_PREFIX.length), P256_SPKI_PREFIX)
  ) {
    throw new RingError("RING_PASSKEY", { details: { reason: "invalid P-256 SPKI" } });
  }
  const point = spkiBytes.subarray(P256_SPKI_PREFIX.length);
  return Object.freeze({
    credentialId: new Uint8Array(credential.rawId),
    publicKey: P256ReaderKey.fromUncompressed(point),
  });
}

/** The challenge is the attestation's SHA-256, the RPC recomputes it. */
export function passkeyReader(passkey: Passkey): RingReadSigner {
  return Object.freeze({
    reader: readerKeyBytes(checkedReaderKey(passkey.publicKey)),
    async sign(message: Uint8Array): Promise<WebAuthnSignature> {
      const credential = (await webAuthn().get({
        publicKey: {
          challenge: bufferSource(sha256(message)),
          allowCredentials: [{ type: "public-key", id: bufferSource(passkey.credentialId) }],
          userVerification: "required",
        },
      })) as PublicKeyCredential | null;
      const response = credential?.response as AuthenticatorAssertionResponse | undefined;
      if (!response) {
        throw new RingError("RING_PASSKEY", { details: { reason: "no assertion" } });
      }
      return Object.freeze({
        signature: new Uint8Array(response.signature),
        authenticatorData: new Uint8Array(response.authenticatorData),
        clientDataJSON: new Uint8Array(response.clientDataJSON),
      });
    },
  });
}

/** WebAuthn wants an `ArrayBuffer`-backed view. */
function bufferSource(bytes: Uint8Array): Uint8Array<ArrayBuffer> {
  return new Uint8Array(bytes);
}

function webAuthn(): CredentialsContainer {
  const credentials = globalThis.navigator?.credentials;
  if (!credentials) {
    throw new RingError("RING_PASSKEY", { details: { reason: "WebAuthn is unavailable" } });
  }
  return credentials;
}
