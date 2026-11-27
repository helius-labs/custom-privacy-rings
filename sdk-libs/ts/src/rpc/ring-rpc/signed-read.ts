import type { Address, Bytes32 } from "@heliuslabs/spr/interface";
import { addressBytes, equalBytes } from "@heliuslabs/spr/internal";
import { RingError } from "../../protocol/error.js";
import { readerKeyFromBytes } from "../../protocol/membership/reader-access.js";
import {
  type RingReadTarget,
  targetLines,
  walletLines,
  checkedReadTarget,
  AttestationRequest,
  authWire,
} from "./signed-request.js";
import {
  type WebAuthnSignature,
  type RingReadSigner,
} from "../../protocol/ring-rpc/read-signer.js";
import {
  base64Decoder,
  encoder,
  checkedUnixTimestamp,
  invalid,
  base64,
  exactRequestRecord,
  invalidRequest,
  requestAddress,
  requestBytes,
} from "./wire.js";
import { defined } from "../../protocol/defined.js";

const READ_DOMAIN = "zolana/ring-rpc-read";
/** Rust `AUDIT_PAGE_LIMIT`. */
export const RING_READ_PAGE_LIMIT = 100n;
/** Rust `AUDIT_CURSOR_LIMIT`. */
export const RING_READ_CURSOR_LIMIT = 256;

/** Rust `PagedRead`, the paged read a reader's signature authorizes. */
export type RingPagedRead = "getDecryptedTransactions" | "getWalletActivity" | "getAuditedMerges";

/** Mirrors Rust `ReadAttestation::message`. */
export function ringReadAttestation(
  input: Readonly<{
    method: RingPagedRead;
    target: RingReadTarget;
    timestamp: bigint;
    nonce: Bytes32;
    cursor?: Uint8Array;
    limit?: bigint;
    wallet?: Address;
  }>,
): Uint8Array {
  const cursor = input.cursor === undefined ? "" : base64Decoder.decode(input.cursor);
  return encoder.encode(
    [
      READ_DOMAIN,
      `method: ${input.method}`,
      ...targetLines(input.target),
      `timestamp: ${input.timestamp}`,
      `nonce: ${base64Decoder.decode(input.nonce)}`,
      `limit: ${input.limit ?? 0n}`,
      `cursor: ${cursor}`,
      ...walletLines(input.wallet),
    ].join("\n"),
  );
}

/** Mirrors Rust `GetDecryptedTransactionsRequest` after `ReadRequest::sign`. */
export interface SignedRingRead {
  readonly method: RingPagedRead;
  readonly ringProgramId: Address;
  readonly cursor?: Uint8Array;
  readonly limit?: bigint;
  /** Narrows the read to the wallet's rows. */
  readonly wallet?: Address;
  readonly reader: Uint8Array;
  readonly timestamp: bigint;
  readonly nonce: Bytes32;
  readonly signature: Uint8Array | WebAuthnSignature;
}

/** Mirrors Rust `ReadRequest`, every signature draws its own nonce. */
export class RingReadRequest extends AttestationRequest {
  readonly #method: RingPagedRead;
  readonly #target: RingReadTarget;
  #cursor?: Uint8Array;
  #limit?: bigint;
  #wallet?: Address;

  private constructor(method: RingPagedRead, target: RingReadTarget) {
    super();
    this.#method = method;
    this.#target = checkedReadTarget(target);
  }

  static decryptedTransactions(target: RingReadTarget): RingReadRequest {
    return new RingReadRequest("getDecryptedTransactions", target);
  }

  static walletActivity(target: RingReadTarget, wallet: Address): RingReadRequest {
    return new RingReadRequest("getWalletActivity", target).withWallet(wallet);
  }

  static auditedMerges(target: RingReadTarget): RingReadRequest {
    return new RingReadRequest("getAuditedMerges", target);
  }

  withCursor(cursor: Uint8Array): this {
    this.#cursor = new Uint8Array(checkedReadCursor(cursor));
    return this;
  }

  withLimit(limit: bigint): this {
    this.#limit = checkedReadLimit(limit);
    return this;
  }

  withWallet(wallet: Address): this {
    addressBytes(wallet, "wallet");
    this.#wallet = wallet;
    return this;
  }

  async sign(signer: RingReadSigner): Promise<SignedRingRead> {
    const { timestamp, nonce } = this.stamp();
    const signature = await signer.sign(
      ringReadAttestation({
        method: this.#method,
        target: this.#target,
        timestamp,
        nonce,
        ...(this.#cursor === undefined ? {} : { cursor: this.#cursor }),
        ...(this.#limit === undefined ? {} : { limit: this.#limit }),
        ...(this.#wallet === undefined ? {} : { wallet: this.#wallet }),
      }),
    );
    return Object.freeze({
      method: this.#method,
      ringProgramId: this.#target.ringProgramId,
      ...(this.#cursor === undefined ? {} : { cursor: this.#cursor }),
      ...(this.#limit === undefined ? {} : { limit: this.#limit }),
      ...(this.#wallet === undefined ? {} : { wallet: this.#wallet }),
      reader: signer.reader,
      timestamp,
      nonce,
      signature,
    });
  }
}

export function signedReadParams(
  checked: ReadAuth & Partial<Pick<SignedRingRead, "cursor" | "limit">>,
): Record<string, unknown> {
  const { signature } = checked;
  return {
    ringProgramId: checked.ringProgramId,
    ...(checked.cursor === undefined ? {} : { cursor: base64Decoder.decode(checked.cursor) }),
    ...(checked.limit === undefined ? {} : { limit: Number(checked.limit) }),
    ...defined({ wallet: checked.wallet }),
    auth: {
      reader: base64Decoder.decode(checked.reader),
      ...authWire({
        timestamp: checked.timestamp,
        nonce: checked.nonce,
        signature: signature instanceof Uint8Array ? signature : signature.signature,
      }),
      ...(signature instanceof Uint8Array
        ? {}
        : {
            webauthn: {
              authenticatorData: base64Decoder.decode(signature.authenticatorData),
              clientDataJson: base64Decoder.decode(signature.clientDataJSON),
            },
          }),
    },
  };
}

/** A response cursor must be usable for the next signed page. */
export function readResponseCursor(
  value: unknown,
  request: Readonly<{ cursor?: Uint8Array | undefined }>,
  path = "result.value.cursor",
): Uint8Array | undefined {
  if (value === null) return undefined;
  if (value === undefined) throw invalid(path);
  const cursor = base64(value, path);
  if (
    cursor.length === 0 ||
    cursor.length > RING_READ_CURSOR_LIMIT ||
    (request.cursor !== undefined && equalBytes(cursor, request.cursor))
  )
    throw invalid(path);
  return cursor;
}

export function checkedReadCursor(cursor: unknown): Uint8Array {
  if (
    !(cursor instanceof Uint8Array) ||
    cursor.length === 0 ||
    cursor.length > RING_READ_CURSOR_LIMIT
  ) {
    throw new RingError("RING_READ_CURSOR", {
      details: { length: cursor instanceof Uint8Array ? cursor.length : -1 },
    });
  }
  return new Uint8Array(cursor);
}

function checkedReadLimit(limit: unknown): bigint {
  if (typeof limit !== "bigint" || limit < 1n || limit > RING_READ_PAGE_LIMIT) {
    throw new RingError("RING_READ_LIMIT", {
      details: { limit: String(limit) },
    });
  }
  return limit;
}

const SIGNED_READ_FIELDS = new Set([
  "method",
  "ringProgramId",
  "cursor",
  "limit",
  "wallet",
  "reader",
  "timestamp",
  "nonce",
  "signature",
]);
const SIGNED_READ_REQUIRED_FIELDS = new Set([
  "method",
  "ringProgramId",
  "reader",
  "timestamp",
  "nonce",
  "signature",
]);
const WEBAUTHN_FIELDS = new Set(["signature", "authenticatorData", "clientDataJSON"]);

function checkedReaderBytes(value: unknown): Uint8Array {
  if (!(value instanceof Uint8Array) || value.length !== 34) {
    throw new RingError("RING_READER_KEY", {
      details: { length: value instanceof Uint8Array ? value.length : -1 },
    });
  }
  const reader = new Uint8Array(value);
  try {
    readerKeyFromBytes(reader);
  } catch (cause) {
    if (cause instanceof RingError) throw cause;
    throw new RingError("RING_READER_KEY", { cause });
  }
  return reader;
}

export function checkedSignedRead(value: unknown, method: RingPagedRead): SignedRingRead {
  const read = exactRequestRecord(
    value,
    SIGNED_READ_FIELDS,
    SIGNED_READ_REQUIRED_FIELDS,
    "request",
  );
  if (read["method"] !== method) throw invalidRequest("method");
  // Rust `ReadScope`, a full transaction page is ring wide and wallet activity names its wallet.
  if (
    method !== "getAuditedMerges" &&
    (method === "getWalletActivity") !== (read["wallet"] !== undefined)
  )
    throw invalidRequest("wallet");
  const cursor = read["cursor"] === undefined ? undefined : checkedReadCursor(read["cursor"]);
  const limit = read["limit"] === undefined ? undefined : checkedReadLimit(read["limit"]);
  return Object.freeze({
    method,
    ...checkedReadAuth(read),
    ...defined({ cursor, limit }),
  });
}

type ReadAuth = Omit<SignedRingRead, "method" | "cursor" | "limit">;

export function checkedReadAuth(read: Readonly<Record<string, unknown>>): ReadAuth {
  const ringProgramId = requestAddress(read["ringProgramId"], "ringProgramId");
  const wallet =
    read["wallet"] === undefined ? {} : { wallet: requestAddress(read["wallet"], "wallet") };
  const reader = checkedReaderBytes(read["reader"]);
  const nonce = requestBytes(read["nonce"], 32, "nonce") as Bytes32;
  const timestamp = checkedUnixTimestamp(read["timestamp"], "timestamp");
  const signature = read["signature"];
  if (signature instanceof Uint8Array) {
    return Object.freeze({
      ringProgramId,
      ...wallet,
      reader,
      nonce,
      timestamp,
      signature: requestBytes(signature, 64, "signature"),
    });
  }
  const webauthn = exactRequestRecord(signature, WEBAUTHN_FIELDS, WEBAUTHN_FIELDS, "signature");
  if (!(webauthn["signature"] instanceof Uint8Array) || webauthn["signature"].length === 0) {
    throw invalidRequest("signature");
  }
  const authenticatorData = webauthn["authenticatorData"];
  if (!(authenticatorData instanceof Uint8Array) || authenticatorData.length < 37) {
    throw invalidRequest("authenticatorData");
  }
  const clientDataJSON = webauthn["clientDataJSON"];
  if (!(clientDataJSON instanceof Uint8Array) || clientDataJSON.length === 0) {
    throw invalidRequest("clientDataJSON");
  }
  return Object.freeze({
    ringProgramId,
    ...wallet,
    reader,
    nonce,
    timestamp,
    signature: Object.freeze({
      signature: new Uint8Array(webauthn["signature"]),
      authenticatorData: new Uint8Array(authenticatorData),
      clientDataJSON: new Uint8Array(clientDataJSON),
    }),
  });
}
