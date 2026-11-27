import { ViewingKey } from "@heliuslabs/spr/keypair";
import type { MessagePartialSigner } from "@solana/kit";
import type { Address, Bytes32, Signature } from "@heliuslabs/spr/interface";
import { addressBytes, type indexerTypes } from "@heliuslabs/spr/internal";
import {
  checkedReaderKey,
  readerKeyBytes,
  type ReaderKey,
} from "../../protocol/membership/reader-access.js";
import {
  encodeMergeSettings,
  shieldedAddressText,
} from "../../protocol/ring-rpc/ring-rpc-codec.js";
import type {
  AuthorityTransfer,
  GetListSaltRequest,
  MergeSettings,
  PrepareAuthorityTransferRequest,
  ReaderGrantAction,
  SetMergeSettingsRequest,
  SetReaderGrantRequest,
} from "../../protocol/ring-rpc/ring-rpc-types.js";
import {
  type RingReadTarget,
  targetLines,
  checkedReadTarget,
  AttestationRequest,
  walletLines,
} from "./signed-request.js";
import { signMessage } from "../../protocol/ring-rpc/read-signer.js";
import {
  encoder,
  base64Decoder,
  requestBytes,
  base58Decoder,
  requestAddress,
  invalidRequest,
} from "./wire.js";
import { defined } from "../../protocol/defined.js";

const AUTHORITY_TRANSFER_DOMAIN = "zolana/ring-rpc-authority-transfer";
const LIST_SALT_DOMAIN = "zolana/ring-rpc-list-salt";
const MERGE_SETTINGS_DOMAIN = "zolana/ring-rpc-merge-settings";
const READER_GRANT_DOMAIN = "zolana/ring-rpc-reader-grant";

/** Mirrors Rust `AuthorityTransferAttestation::message`, one `payment:` line per payment in order. */
export function authorityTransferAttestation(
  input: Readonly<{
    target: RingReadTarget;
    delegate: Address;
    transfer: AuthorityTransfer;
    timestamp: bigint;
    nonce: Bytes32;
  }>,
): Uint8Array {
  return encoder.encode(
    [
      AUTHORITY_TRANSFER_DOMAIN,
      ...targetLines(input.target),
      `delegate: ${input.delegate}`,
      ...input.transfer.sources.map((source) => `source: ${shieldedAddressText(source)}`),
      ...(
        [
          ["payment", input.transfer.payments],
          ["exit", input.transfer.exits],
        ] as const
      ).flatMap(([kind, outputs]) =>
        outputs.map(
          (output) =>
            `${kind}: ${shieldedAddressText(output.recipient)} ${output.mint} ${output.amount} ${output.tag === undefined ? "-" : base58Decoder.decode(output.tag)}`,
        ),
      ),
      ...input.transfer.withdrawals.map(
        (withdrawal) =>
          `withdrawal: ${withdrawal.recipient} ${withdrawal.mint} ${withdrawal.amount} ${withdrawal.tokenProgram ?? "-"}`,
      ),
      `timestamp: ${input.timestamp}`,
      `nonce: ${base64Decoder.decode(input.nonce)}`,
    ].join("\n"),
  );
}

/** Mirrors Rust `AuthorityTransferRequest`, the ring delegate signs it. */
export class RingAuthorityTransferRequest extends AttestationRequest {
  readonly #target: RingReadTarget;
  readonly #transfer: AuthorityTransfer;
  #txViewingKey?: Bytes32;

  private constructor(target: RingReadTarget, transfer: AuthorityTransfer) {
    super();
    this.#target = checkedReadTarget(target);
    this.#transfer = transfer;
  }

  static forTransfer(
    target: RingReadTarget,
    transfer: AuthorityTransfer,
  ): RingAuthorityTransferRequest {
    return new RingAuthorityTransferRequest(target, transfer);
  }

  /** A fresh key when absent. */
  withTxViewingKey(txViewingKey: Bytes32): this {
    this.#txViewingKey = requestBytes(txViewingKey, 32, "txViewingKey") as Bytes32;
    return this;
  }

  async sign(delegate: MessagePartialSigner): Promise<PrepareAuthorityTransferRequest> {
    const { timestamp, nonce } = this.stamp();
    const signature = await signMessage(
      delegate,
      authorityTransferAttestation({
        target: this.#target,
        delegate: delegate.address,
        transfer: this.#transfer,
        timestamp,
        nonce,
      }),
    );
    return Object.freeze({
      ring: this.#target.ringProgramId,
      delegate: delegate.address,
      transfer: this.#transfer,
      txViewingKey:
        this.#txViewingKey === undefined
          ? freshTxViewingKey()
          : (this.#txViewingKey.slice() as Bytes32),
      timestamp,
      nonce: base64Decoder.decode(nonce) as indexerTypes.Base64String,
      signature: base58Decoder.decode(signature) as Signature,
    });
  }
}

function freshTxViewingKey(): Bytes32 {
  const key = ViewingKey.generate();
  const secret = key.secretBytes();
  key.destroy();
  return secret;
}

/** Mirrors Rust `ListSaltAttestation::message`. */
export function listSaltAttestation(
  input: Readonly<{
    target: RingReadTarget;
    writer: Address;
    owner: Address;
    timestamp: bigint;
    nonce: Bytes32;
  }>,
): Uint8Array {
  return encoder.encode(
    [
      LIST_SALT_DOMAIN,
      ...targetLines(input.target),
      `writer: ${input.writer}`,
      `owner: ${input.owner}`,
      `timestamp: ${input.timestamp}`,
      `nonce: ${base64Decoder.decode(input.nonce)}`,
    ].join("\n"),
  );
}

/** Mirrors Rust `ListSaltRequest`, a writer of the ring's salted lists signs it. */
export class RingListSaltRequest extends AttestationRequest {
  readonly #target: RingReadTarget;
  readonly #owner: Address;

  private constructor(target: RingReadTarget, owner: Address) {
    super();
    this.#target = checkedReadTarget(target);
    this.#owner = requestAddress(owner, "owner");
  }

  static forOwner(target: RingReadTarget, owner: Address): RingListSaltRequest {
    return new RingListSaltRequest(target, owner);
  }

  async sign(writer: MessagePartialSigner): Promise<GetListSaltRequest> {
    const { timestamp, nonce } = this.stamp();
    const signature = await signMessage(
      writer,
      listSaltAttestation({
        target: this.#target,
        writer: writer.address,
        owner: this.#owner,
        timestamp,
        nonce,
      }),
    );
    return Object.freeze({
      ring: this.#target.ringProgramId,
      writer: writer.address,
      owner: this.#owner,
      timestamp,
      nonce: base64Decoder.decode(nonce) as indexerTypes.Base64String,
      signature: base58Decoder.decode(signature) as Signature,
    });
  }
}

/** Rust `ReaderGrantAttestation` without its target, a ring-wide grant names no `wallet`. */
export interface RingReaderGrant {
  readonly wallet?: Address;
  readonly reader: ReaderKey;
  readonly action: ReaderGrantAction;
}

/** Mirrors Rust `ReaderGrantAttestation::message`. */
export function readerGrantAttestation(
  input: RingReaderGrant & Readonly<{ target: RingReadTarget; timestamp: bigint; nonce: Bytes32 }>,
): Uint8Array {
  return encoder.encode(
    [
      READER_GRANT_DOMAIN,
      ...targetLines(input.target),
      ...walletLines(input.wallet),
      `reader: ${base64Decoder.decode(readerKeyBytes(checkedReaderKey(input.reader)))}`,
      `action: ${input.action}`,
      `timestamp: ${input.timestamp}`,
      `nonce: ${base64Decoder.decode(input.nonce)}`,
    ].join("\n"),
  );
}

/** Mirrors Rust `ReaderGrantRequest`, the service takes a ring-wide grant only from the ring's policy authority. */
export class RingReaderGrantRequest extends AttestationRequest {
  readonly #target: RingReadTarget;
  readonly #grant: RingReaderGrant;

  private constructor(target: RingReadTarget, grant: RingReaderGrant) {
    super();
    this.#target = checkedReadTarget(target);
    if (grant.wallet !== undefined) addressBytes(grant.wallet, "wallet");
    this.#grant = Object.freeze({
      ...grant,
      reader: checkedReaderKey(grant.reader),
    });
  }

  static forGrant(target: RingReadTarget, grant: RingReaderGrant): RingReaderGrantRequest {
    return new RingReaderGrantRequest(target, grant);
  }

  async sign(signer: MessagePartialSigner): Promise<SetReaderGrantRequest> {
    const { wallet, reader, action } = this.#grant;
    if (wallet !== undefined && signer.address !== wallet) throw invalidRequest("signer");
    const { timestamp, nonce } = this.stamp();
    const signature = await signMessage(
      signer,
      readerGrantAttestation({
        target: this.#target,
        ...this.#grant,
        timestamp,
        nonce,
      }),
    );
    return Object.freeze({
      ring: this.#target.ringProgramId,
      ...defined({ wallet }),
      reader: base64Decoder.decode(readerKeyBytes(reader)) as indexerTypes.Base64String,
      action,
      timestamp,
      nonce: base64Decoder.decode(nonce) as indexerTypes.Base64String,
      signature: base58Decoder.decode(signature) as Signature,
    });
  }
}

/** Mirrors Rust `MergeSettingsAttestation::message`. */
export function mergeSettingsAttestation(
  input: Readonly<{
    target: RingReadTarget;
    owner: Address;
    settings: MergeSettings;
    timestamp: bigint;
    nonce: Bytes32;
  }>,
): Uint8Array {
  return encoder.encode(
    [
      MERGE_SETTINGS_DOMAIN,
      ...targetLines(input.target),
      `owner: ${input.owner}`,
      `mint: ${input.settings.mint}`,
      `target notes: ${input.settings.targetNoteCount}`,
      `minimum average: ${input.settings.minAverageAmount}`,
      `timestamp: ${input.timestamp}`,
      `nonce: ${base64Decoder.decode(input.nonce)}`,
    ].join("\n"),
  );
}

/** Mirrors Rust `MergeSettingsRequest`, the signer is the owner whose notes Ring RPC merges. */
export class RingMergeSettingsRequest extends AttestationRequest {
  readonly #target: RingReadTarget;
  readonly #settings: MergeSettings;

  private constructor(target: RingReadTarget, settings: MergeSettings) {
    super();
    this.#target = checkedReadTarget(target);
    // The wire codec refuses a value the Rust request cannot carry before any signature.
    encodeMergeSettings(settings);
    this.#settings = Object.freeze({ ...settings });
  }

  /** A zero `targetNoteCount` stops managed merges of the mint. */
  static forSettings(target: RingReadTarget, settings: MergeSettings): RingMergeSettingsRequest {
    return new RingMergeSettingsRequest(target, settings);
  }

  async sign(owner: MessagePartialSigner): Promise<SetMergeSettingsRequest> {
    const { timestamp, nonce } = this.stamp();
    const signature = await signMessage(
      owner,
      mergeSettingsAttestation({
        target: this.#target,
        owner: owner.address,
        settings: this.#settings,
        timestamp,
        nonce,
      }),
    );
    return Object.freeze({
      ring: this.#target.ringProgramId,
      owner: owner.address,
      settings: this.#settings,
      timestamp,
      nonce: base64Decoder.decode(nonce) as indexerTypes.Base64String,
      signature: base58Decoder.decode(signature) as Signature,
    });
  }
}
