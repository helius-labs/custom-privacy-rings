import {
  checkedApiKey,
  serviceUrl,
  postJsonRpc,
  TransportFailure,
  checkedEndpoint,
  checkedFetch,
  addressBytes,
  ZERO_32,
  equalBytes,
  indexerScalars,
} from "@heliuslabs/spr/internal";
import { ed25519 } from "@noble/curves/ed25519.js";
import {
  getCompiledTransactionMessageDecoder,
  getTransactionDecoder,
  type MessagePartialSigner,
} from "@solana/kit";
import type { Address, Bytes32, RequestContext } from "@heliuslabs/spr/interface";
import { initializePoseidon } from "@heliuslabs/spr";
import { ringIdField } from "../../crypto/derivation/ring-stream.js";
import { isSmallOrder } from "../../crypto/keys/x25519.js";
import { snapshotPages } from "../wallet-snapshot.js";
import { knownUtxoHash } from "../../operations/audit/audit.js";
import { RingError } from "../../protocol/error.js";
import type { OwnerListSalts } from "../../protocol/membership/private-list.js";
import { decodeRingRpcHealth, type RingRpcHealth } from "./health.js";
import { ringRecordsAddress } from "../../protocol/pda/index.js";
import { verifyRecordUpdate } from "../../protocol/ring-rpc/record-update.js";
import {
  decodeCoSignListEntry,
  decodeCoSignListsPage,
  decodeCoSignTransactionResponse,
  decodeMergeSettings,
  decodeSubmitEnrollmentResponse,
  decodePendingListApprovalsPage,
  decodeRecordsRegistry,
  decodeRecordUpdateStatus,
  decodeRegisterEnrollmentRingResponse,
  decodeRelayQuote,
  decodeRelayStatus,
  decodeReaderGrantAction,
  decodeRelayStep,
  decodeSubmitRecordUpdate,
  encodeCoSignTransactionRequest,
  encodeCreateRecordsRegistryRequest,
  encodeEditCoSignListRequest,
  encodeGetCoSignListsRequest,
  encodeGetPendingListApprovalsRequest,
  encodeGetListSaltRequest,
  encodePrepareAuthorityTransferRequest,
  encodePrepareRelayRequest,
  encodeRecordUpdateStatusRequest,
  encodeRefuseListApprovalRequest,
  encodeRegisterEnrollmentRingRequest,
  encodeRelayQuoteRequest,
  encodeRelayStatusRequest,
  encodeSetMergeSettingsRequest,
  encodeSubmitEnrollmentRequest,
  encodeSetReaderGrantRequest,
  encodeSubmitRecordUpdateRequest,
  encodeSubmitRelayRequest,
} from "../../protocol/ring-rpc/ring-rpc-codec.js";
import {
  RingRpcMethod,
  type CoSignListEntry,
  type CoSignListsPage,
  type CoSignTransactionRequest,
  type CoSignTransactionResponse,
  type CreateRecordsRegistryRequest,
  type EditCoSignListRequest,
  type GetCoSignListsRequest,
  type GetPendingListApprovalsRequest,
  type PendingListApprovalsPage,
  type GetListSaltRequest,
  type MergeSettings,
  type PrepareAuthorityTransferRequest,
  type PrepareRelayRequest,
  type ReaderGrantAction,
  type RecordsRegistry,
  type RecordUpdateStatus,
  type RefuseListApprovalRequest,
  type RegisterEnrollmentRingRequest,
  type RelayQuote,
  type RelayQuoteRequest,
  type RelayStatus,
  type RelayStep,
  type SetMergeSettingsRequest,
  type SubmitEnrollmentRequest,
  type SetReaderGrantRequest,
  type SubmitRecordUpdateRequest,
  type SubmitRelayRequest,
} from "../../protocol/ring-rpc/ring-rpc-types.js";
import { ringRpcRefusalName } from "../../protocol/ring-rpc/ring-rpc-refusal.js";
import {
  type RingAuditorKey,
  RingAuditorKeyRequest,
  type SignedAuditorKeyRequest,
  checkedSignedAuditorKeyRequest,
  auditorKeyStatement,
} from "./auditor-key.js";
import {
  requestAddress,
  record,
  base58Decoder,
  x25519Key,
  invalid,
  address,
  base58,
  hash,
  string,
  type Hash,
  base58Encoder,
  base64Decoder,
  exactRequestRecord,
  list,
  u64,
  signedInteger,
  signature,
  u16,
  u32,
  exactRecord,
  canonicalHash,
  invalidRequest,
} from "./wire.js";
import {
  authWire,
  type RingReadTarget,
  checkedReadTarget,
  timestampOrNow,
  freshNonce,
} from "./signed-request.js";
import { type RingReadSigner, signMessage } from "../../protocol/ring-rpc/read-signer.js";
import { type RingStatus, decodeActivation } from "./status.js";
import {
  type DecryptedRingTransactionsPage,
  type RingAuditedUtxosPage,
  RingAuditedUtxosRequest,
  type SignedAuditedUtxosRead,
  SIGNED_UTXOS_FIELDS,
  SIGNED_UTXOS_REQUIRED_FIELDS,
  checkedUtxoHashes,
  type AuditedMergesPage,
} from "./audit-reads.js";
import {
  RingReadRequest,
  checkedReadCursor,
  checkedReadAuth,
  signedReadParams,
  type SignedRingRead,
  checkedSignedRead,
  readResponseCursor,
  RING_READ_PAGE_LIMIT,
} from "./signed-read.js";
import {
  type RingWalletActivityPage,
  type RingWalletStatusQuery,
  type RingWalletStatusPage,
  type RingDelegateNotesQuery,
  type RingDelegateNotesPage,
  type RingSpendCoverage,
  spendCoverageAttestation,
  checkedSnapshotCursor,
  walletStatusAttestation,
  walletSnapshotReleaseAttestation,
  delegateNotesAttestation,
  delegateSnapshotReleaseAttestation,
} from "./wallet-reads.js";
import {
  decodeWalletStatus,
  decodeSpendCoverage,
  decodeWalletActivity,
} from "./wallet-decoders.js";
import {
  decodeKnownUtxo,
  AUDITED_MERGES_PAGE_FIELDS,
  decodeAuditedMerge,
  decodeTransaction,
  decodeSkipped,
} from "./audit-decoders.js";
import { defined } from "../../protocol/defined.js";

const RING_RPC_TIMEOUT_MS = 30_000;
const RING_RPC_MAX_REQUEST_BYTES = 1024 * 1024;
const RING_RPC_MAX_RESPONSE_BYTES = 4 * 1024 * 1024;

export interface RingRpcOptions {
  readonly fetch?: typeof globalThis.fetch;
  /** Even loopback needs it for plain HTTP. */
  readonly allowInsecureHttp?: boolean;
  /** Gateway API key. Supply it here or in the URL's `api-key` query parameter, never both. */
  readonly apiKey?: string;
}

/**
 * Ring RPC over HTTPS. Pass a service base URL, such as `https://host/v1/zolana/ring`;
 * each call appends its method name. API keys are omitted from {@link RingRpc.url}.
 * Signed reader and authority requests still require their own authorization.
 */
export class RingRpc {
  readonly #url: URL;
  readonly #apiKey?: string;
  readonly #fetch: typeof globalThis.fetch;

  constructor(url: string | URL, options?: RingRpcOptions) {
    try {
      const keyed = checkedApiKey(
        checkedEndpoint(url, {
          field: "url",
          ...(options?.allowInsecureHttp === undefined
            ? {}
            : { allowInsecureHttp: options.allowInsecureHttp }),
        }),
        options?.apiKey,
      );
      this.#url = keyed.endpoint;
      if (keyed.apiKey !== undefined) this.#apiKey = keyed.apiKey;
      this.#fetch = checkedFetch(options?.fetch);
    } catch (error) {
      if (!(error instanceof TransportFailure)) throw error;
      throw new RingError("RING_RPC_CONFIG", { details: { ...error.facts } });
    }
  }

  get url(): string {
    return this.#url.href;
  }

  async health(context?: RequestContext): Promise<RingRpcHealth> {
    return decodeRingRpcHealth(await this.#call("health", undefined, context));
  }

  /** Verified under the pinned `servicePublicKey` before the key is returned. */
  async createAuditorKey(
    input: Readonly<{
      ringProgramId: Address;
      genesisHash: Bytes32;
      /** The upgrade authority, or the policy authority after `Initialize`. */
      authority: MessagePartialSigner;
      /** Confirmed out of band, never read from the service. */
      servicePublicKey: Address;
      timestamp?: bigint;
    }>,
    context?: RequestContext,
  ): Promise<RingAuditorKey> {
    let request = RingAuditorKeyRequest.forRing(input.ringProgramId, input.genesisHash);
    if (input.timestamp !== undefined) request = request.at(input.timestamp);
    return this.createAuditorKeySigned(
      await request.sign(input.authority),
      input.servicePublicKey,
      context,
    );
  }

  /** Refused unless the pinned `servicePublicKey` signed the answer. */
  async createAuditorKeySigned(
    request: SignedAuditorKeyRequest,
    servicePublicKey: Address,
    context?: RequestContext,
  ): Promise<RingAuditorKey> {
    const checked = checkedSignedAuditorKeyRequest(request);
    const pinned = requestAddress(servicePublicKey, "servicePublicKey");
    const wire = record(
      await this.#call(
        "createAuditorKey",
        {
          ringProgramId: checked.ringProgramId,
          auth: {
            authority: checked.authority,
            genesisHash: base58Decoder.decode(checked.genesisHash),
            ...authWire(checked),
          },
        },
        context,
      ),
      "result",
    );
    const auditorPublicKey = x25519Key(wire["auditorPubkey"], "result.auditorPubkey");
    if (isSmallOrder(auditorPublicKey)) throw invalid("result.auditorPubkey");
    if (address(wire["servicePubkey"], "result.servicePubkey") !== pinned) {
      throw new RingError("RING_RPC", {
        details: {
          method: "createAuditorKey",
          reason: "service key is not the pinned key",
        },
      });
    }
    const signature = base58(wire["signature"], "result.signature");
    const ring = address(wire["ringProgramId"], "result.ringProgramId");
    if (ring !== checked.ringProgramId) throw invalid("result.ringProgramId");
    const auditorViewTag = hash(wire["auditorViewTag"], "result.auditorViewTag");
    if (!equalBytes(auditorViewTag, auditorPublicKey.u())) throw invalid("result.auditorViewTag");
    const attested =
      signature.length === 64 &&
      ed25519.verify(
        signature,
        auditorKeyStatement({
          cluster: checked.genesisHash,
          ringProgramId: ring,
          auditorPublicKey,
          nonce: checked.nonce,
        }),
        addressBytes(pinned, "servicePublicKey"),
        { zip215: false },
      );
    if (!attested) {
      throw new RingError("RING_RPC", {
        details: {
          method: "createAuditorKey",
          reason: "attestation signature is invalid",
        },
      });
    }
    return Object.freeze({
      ringProgramId: ring,
      auditorPublicKey,
      auditorViewTag,
      servicePublicKey: pinned,
      signature,
    });
  }

  /** Whether this service can open the ring, before a read is attempted. */
  async ringStatus(ringProgramId: Address, context?: RequestContext): Promise<RingStatus> {
    const expectedRing = requestAddress(ringProgramId, "ringProgramId");
    const wire = record(
      await this.#call("ringStatus", { ringProgramId: expectedRing }, context),
      "result",
    );
    const state = string(wire["state"], "result.state");
    if (state !== "served" && state !== "foreignAuditor" && state !== "uninitialized") {
      throw invalid("result.state");
    }
    const ring = address(wire["ringProgramId"], "result.ringProgramId");
    if (ring !== expectedRing) throw invalid("result.ringProgramId");
    const config = wire["configAuditorPubkey"];
    const activation = wire["activation"];
    return Object.freeze({
      ringProgramId: ring,
      state,
      ...(config === undefined
        ? {}
        : {
            configAuditorPublicKey: x25519Key(config, "result.configAuditorPubkey"),
          }),
      servicePublicKey: address(wire["servicePubkey"], "result.servicePubkey"),
      ...(activation === undefined ? {} : { activation: decodeActivation(activation) }),
    });
  }

  /** Refused before sending unless the signature verifies, the service must answer the same id. */
  async submitRecordUpdate(
    request: SubmitRecordUpdateRequest,
    context?: RequestContext,
  ): Promise<Hash> {
    const id = indexerScalars.hash(verifyRecordUpdate(request));
    const wire = await this.#call(
      RingRpcMethod.submitRecordUpdate,
      encodeSubmitRecordUpdateRequest(request),
      context,
    );
    if (decodeSubmitRecordUpdate(wire) !== id) throw invalid("result.updateId");
    return id;
  }

  async getRecordUpdateStatus(
    updateId: Hash,
    context?: RequestContext,
  ): Promise<RecordUpdateStatus> {
    return decodeRecordUpdateStatus(
      await this.#call(
        RingRpcMethod.getRecordUpdateStatus,
        encodeRecordUpdateStatusRequest(updateId),
        context,
      ),
    );
  }

  /** One page of the ring's updates pending list approval, oldest first. */
  async getPendingListApprovals(
    request: GetPendingListApprovalsRequest,
    context?: RequestContext,
  ): Promise<PendingListApprovalsPage> {
    return decodePendingListApprovalsPage(
      await this.#call(
        RingRpcMethod.getPendingListApprovals,
        encodeGetPendingListApprovalsRequest(request),
        context,
      ),
    );
  }

  async refuseListApproval(
    request: RefuseListApprovalRequest,
    context?: RequestContext,
  ): Promise<RecordUpdateStatus> {
    return decodeRecordUpdateStatus(
      await this.#call(
        RingRpcMethod.refuseListApproval,
        encodeRefuseListApprovalRequest(request),
        context,
      ),
    );
  }

  /** The service's signature over the message of `transaction`, for a wallet sending it itself. */
  async coSignTransaction(
    request: CoSignTransactionRequest,
    context?: RequestContext,
  ): Promise<CoSignTransactionResponse> {
    const { messageBytes } = getTransactionDecoder().decode(
      indexerScalars.base64Bytes(request.transaction),
    );
    const message = new Uint8Array(messageBytes);
    const response = decodeCoSignTransactionResponse(
      await this.#call(
        RingRpcMethod.coSignTransaction,
        encodeCoSignTransactionRequest(request),
        context,
      ),
    );
    const sig = new Uint8Array(base58Encoder.encode(response.signature));
    if (
      sig.length !== 64 ||
      !ed25519.verify(sig, message, addressBytes(response.signer, "signer"), { zip215: false })
    ) {
      throw new RingError("RING_RPC", {
        details: {
          method: RingRpcMethod.coSignTransaction,
          reason: "invalid cosignature",
        },
      });
    }
    return response;
  }

  /** Answered with the entry the edit set or removed. */
  async editCoSignList(
    request: EditCoSignListRequest,
    context?: RequestContext,
  ): Promise<CoSignListEntry> {
    return decodeCoSignListEntry(
      await this.#call(RingRpcMethod.editCoSignList, encodeEditCoSignListRequest(request), context),
    );
  }

  /** One page of the lists the ring's co-signer screens against. */
  async getCoSignLists(
    request: GetCoSignListsRequest,
    context?: RequestContext,
  ): Promise<CoSignListsPage> {
    return decodeCoSignListsPage(
      await this.#call(RingRpcMethod.getCoSignLists, encodeGetCoSignListsRequest(request), context),
    );
  }

  /** The ring's records under the service's storage authority, its signature over the request message while they do not exist. */
  async createRecordsRegistry(
    request: CreateRecordsRegistryRequest,
    context?: RequestContext,
  ): Promise<RecordsRegistry> {
    const message = indexerScalars.base64Bytes(request.message);
    const { header, staticAccounts } = getCompiledTransactionMessageDecoder().decode(message);
    const records = decodeRecordsRegistry(
      await this.#call(
        RingRpcMethod.createRecordsRegistry,
        encodeCreateRecordsRegistryRequest(request),
        context,
      ),
    );
    const refused = (reason: string) =>
      new RingError("RING_RPC", {
        details: { method: RingRpcMethod.createRecordsRegistry, reason },
      });
    const { storageAuthority, signature } = records;
    if (records.registry !== (await ringRecordsAddress(request.ring, storageAuthority)))
      throw refused("registry is not the ring records under storageAuthority");
    if (!staticAccounts.slice(1, header.numSignerAccounts).includes(storageAuthority))
      throw refused("storageAuthority does not create");
    if (signature === undefined) return records;
    const signatureBytes = new Uint8Array(base58Encoder.encode(signature));
    if (
      signatureBytes.length !== 64 ||
      !ed25519.verify(signatureBytes, message, addressBytes(storageAuthority, "storageAuthority"), {
        zip215: false,
      })
    )
      throw refused("invalid storage authority signature");
    return records;
  }

  /** Ring RPC's enrollment writer serves the ring from then on, while the ring policy names its key. */
  async registerEnrollmentRing(
    request: RegisterEnrollmentRingRequest,
    context?: RequestContext,
  ): Promise<void> {
    decodeRegisterEnrollmentRingResponse(
      await this.#call(
        RingRpcMethod.registerEnrollmentRing,
        encodeRegisterEnrollmentRingRequest(request),
        context,
      ),
    );
  }

  /** Each page is signed again, the attestation binds the nonce, the cursor and the time. */
  async getDecryptedTransactions(
    input: Readonly<{
      target: RingReadTarget;
      signer: RingReadSigner;
      cursor?: Uint8Array;
      limit?: bigint;
      timestamp?: bigint;
    }>,
    context?: RequestContext,
  ): Promise<DecryptedRingTransactionsPage> {
    let request = RingReadRequest.decryptedTransactions(input.target);
    if (input.cursor !== undefined) request = request.withCursor(input.cursor);
    if (input.limit !== undefined) request = request.withLimit(input.limit);
    if (input.timestamp !== undefined) request = request.at(input.timestamp);
    return this.readSigned(await request.sign(input.signer), context);
  }

  /** The wallet's own part of each transaction it spends in or receives an output in. */
  async getWalletActivity(
    input: Readonly<{
      target: RingReadTarget;
      signer: RingReadSigner;
      wallet: Address;
      cursor?: Uint8Array;
      limit?: bigint;
      timestamp?: bigint;
    }>,
    context?: RequestContext,
  ): Promise<RingWalletActivityPage> {
    let request = RingReadRequest.walletActivity(input.target, input.wallet);
    if (input.cursor !== undefined) request = request.withCursor(input.cursor);
    if (input.limit !== undefined) request = request.withLimit(input.limit);
    if (input.timestamp !== undefined) request = request.at(input.timestamp);
    return this.readSignedWalletActivity(await request.sign(input.signer), context);
  }

  /** The wallet's notes and balance as of the slot the operator verified. */
  async getWalletStatus(
    input: RingWalletStatusQuery,
    context?: RequestContext,
  ): Promise<RingWalletStatusPage> {
    const wire = await this.#walletSnapshotCall({ input, method: "getWalletStatus", context });
    return decodeWalletStatus(record(wire, "result"));
  }

  async releaseWalletSnapshot(
    input: RingWalletStatusQuery & Readonly<{ cursor: Uint8Array }>,
    context?: RequestContext,
  ): Promise<void> {
    const released = await this.#walletSnapshotCall({
      input,
      method: "releaseWalletSnapshot",
      context,
    });
    if (released !== true) throw invalid("result");
  }

  walletStatusPages(
    input: Omit<RingWalletStatusQuery, "cursor" | "timestamp">,
    context?: RequestContext,
  ): AsyncGenerator<RingWalletStatusPage, void> {
    return snapshotPages({
      read: (cursor) => this.getWalletStatus({ ...input, ...defined({ cursor }) }, context),
      status: (page) => page,
      release: (cursor, cleanup) => this.releaseWalletSnapshot({ ...input, cursor }, cleanup),
      ...defined({ context }),
    });
  }

  async getDelegateNotes(
    input: RingDelegateNotesQuery,
    context?: RequestContext,
  ): Promise<RingDelegateNotesPage> {
    const wire = record(
      await this.#delegateSnapshotCall({ input, method: "getDelegateNotes", context }),
      "result",
    );
    const nullifierSecret =
      wire["nullifierSecret"] == null
        ? undefined
        : hash(wire["nullifierSecret"], "nullifierSecret");
    if (nullifierSecret !== undefined && nullifierSecret[0] !== 0) throw invalid("nullifierSecret");
    return Object.freeze({
      ...defined({ nullifierSecret }),
      status: decodeWalletStatus(record(wire["status"], "status")),
    });
  }

  async releaseDelegateSnapshot(
    input: RingDelegateNotesQuery & Readonly<{ cursor: Uint8Array }>,
    context?: RequestContext,
  ): Promise<void> {
    const released = await this.#delegateSnapshotCall({
      input,
      method: "releaseDelegateSnapshot",
      context,
    });
    if (released !== true) throw invalid("result");
  }

  delegateNotesPages(
    input: Omit<RingDelegateNotesQuery, "cursor" | "timestamp">,
    context?: RequestContext,
  ): AsyncGenerator<RingDelegateNotesPage, void> {
    return snapshotPages({
      read: (cursor) => this.getDelegateNotes({ ...input, ...defined({ cursor }) }, context),
      status: (page) => page.status,
      release: (cursor, cleanup) => this.releaseDelegateSnapshot({ ...input, cursor }, cleanup),
      ...defined({ context }),
    });
  }

  /** The ring authority's read of unresolved spends, no wallet read carries them. */
  async getSpendCoverage(
    input: Readonly<{
      target: RingReadTarget;
      authority: MessagePartialSigner;
      timestamp?: bigint;
      cursor?: Uint8Array;
    }>,
    context?: RequestContext,
  ): Promise<RingSpendCoverage> {
    const target = checkedReadTarget(input.target);
    const timestamp = timestampOrNow(input.timestamp);
    const nonce = freshNonce();
    const cursor = input.cursor === undefined ? undefined : checkedReadCursor(input.cursor);
    const signed = await signMessage(
      input.authority,
      spendCoverageAttestation({
        target,
        authority: input.authority.address,
        timestamp,
        nonce,
        ...defined({ cursor }),
      }),
    );
    const wire = record(
      await this.#call(
        "getSpendCoverage",
        {
          ring: target.ringProgramId,
          authority: input.authority.address,
          timestamp: Number(timestamp),
          nonce: base64Decoder.decode(nonce),
          ...(cursor === undefined ? {} : { cursor: base64Decoder.decode(cursor) }),
          signature: base58Decoder.decode(signed),
        },
        context,
      ),
      "result",
    );
    return decodeSpendCoverage(wire);
  }

  /** The UTXOs the service opened, signed over the requested hashes. */
  async getAuditedUtxos(
    input: Readonly<{
      target: RingReadTarget;
      utxoHashes: readonly Bytes32[];
      signer: RingReadSigner;
      wallet?: Address;
      timestamp?: bigint;
    }>,
    context?: RequestContext,
  ): Promise<RingAuditedUtxosPage> {
    let request = RingAuditedUtxosRequest.read(input.target, input.utxoHashes);
    if (input.wallet !== undefined) request = request.withWallet(input.wallet);
    if (input.timestamp !== undefined) request = request.at(input.timestamp);
    return this.readSignedUtxos(await request.sign(input.signer), context);
  }

  /** Rust `GetAuditedUtxosResponse::known_utxos`, an answer is used only where it rebuilds its requested hash. */
  async readSignedUtxos(
    read: SignedAuditedUtxosRead,
    context?: RequestContext,
  ): Promise<RingAuditedUtxosPage> {
    await initializePoseidon();
    const { utxoHashes, ...auth } = exactRequestRecord(
      read,
      SIGNED_UTXOS_FIELDS,
      SIGNED_UTXOS_REQUIRED_FIELDS,
      "request",
    );
    const hashes = checkedUtxoHashes(utxoHashes);
    const checked = checkedReadAuth(auth);
    const wire = record(
      await this.#call(
        "getAuditedUtxos",
        {
          ...signedReadParams(checked),
          utxoHashes: hashes.map((utxoHash) => base58Decoder.decode(utxoHash)),
        },
        context,
      ),
      "result",
    );
    const wireContext = record(wire["context"], "result.context");
    const utxos = list(record(wire["value"], "result.value")["utxos"], "result.value.utxos");
    if (utxos.length !== hashes.length) throw invalid("result.value.utxos");
    const ringId = ringIdField(checked.ringProgramId);
    return Object.freeze({
      slot: u64(wireContext["slot"], "result.context.slot"),
      blockTime: signedInteger(wireContext["blockTime"], "result.context.blockTime"),
      utxos: Object.freeze(
        utxos.map((entry, index) => {
          if (entry === null) return undefined;
          const path = `result.value.utxos[${index}]`;
          const item = record(entry, path);
          const utxoHash = hash(item["utxoHash"], `${path}.utxoHash`);
          const utxo = decodeKnownUtxo(record(item["utxo"], `${path}.utxo`), `${path}.utxo`);
          // The ring's UTXO or a default-ring one the ring spent, any other opening is not this hash's.
          if (
            !equalBytes(utxoHash, hashes[index] ?? new Uint8Array()) ||
            ![ringId, ZERO_32].some((ring) => {
              const rebuilt = knownUtxoHash(utxo, ring);
              return rebuilt !== undefined && equalBytes(rebuilt, utxoHash);
            })
          )
            throw invalid(`${path}.utxo`);
          return Object.freeze({
            utxoHash,
            utxo,
            slot: u64(item["slot"], `${path}.slot`),
            signature: signature(item["txSignature"], `${path}.txSignature`),
            eventIndex: u16(item["eventIndex"], `${path}.eventIndex`),
            slotIndex: u32(item["slotIndex"], `${path}.slotIndex`),
          });
        }),
      ),
    });
  }

  /** Merge records require finalized execution. */
  async getAuditedMerges(
    input: Readonly<{
      target: RingReadTarget;
      signer: RingReadSigner;
      cursor?: Uint8Array;
      limit?: bigint;
      wallet?: Address;
      timestamp?: bigint;
    }>,
    context?: RequestContext,
  ): Promise<AuditedMergesPage> {
    let request = RingReadRequest.auditedMerges(input.target);
    if (input.cursor !== undefined) request = request.withCursor(input.cursor);
    if (input.limit !== undefined) request = request.withLimit(input.limit);
    if (input.wallet !== undefined) request = request.withWallet(input.wallet);
    if (input.timestamp !== undefined) request = request.at(input.timestamp);
    return this.readSignedMerges(await request.sign(input.signer), context);
  }

  async readSignedMerges(
    read: SignedRingRead,
    context?: RequestContext,
  ): Promise<AuditedMergesPage> {
    const checked = checkedSignedRead(read, "getAuditedMerges");
    const wire = record(
      await this.#call("getAuditedMerges", signedReadParams(checked), context),
      "result",
    );
    const wireContext = record(wire["context"], "result.context");
    const value = exactRecord(
      wire["value"],
      AUDITED_MERGES_PAGE_FIELDS,
      AUDITED_MERGES_PAGE_FIELDS,
      "result.value",
    );
    const cursor = readResponseCursor(value["cursor"], checked);
    const opened = list(value["opened"], "result.value.opened");
    if (opened.length > Number(checked.limit ?? RING_READ_PAGE_LIMIT))
      throw invalid("result.value");
    return Object.freeze({
      slot: u64(wireContext["slot"], "result.context.slot"),
      blockTime: signedInteger(wireContext["blockTime"], "result.context.blockTime"),
      opened: Object.freeze(
        opened.map((entry, index) => decodeAuditedMerge(entry, `result.value.opened[${index}]`)),
      ),
      ...defined({ cursor }),
    });
  }

  async readSigned(
    read: SignedRingRead,
    context?: RequestContext,
  ): Promise<DecryptedRingTransactionsPage> {
    const checked = checkedSignedRead(read, "getDecryptedTransactions");
    const wire = record(
      await this.#call("getDecryptedTransactions", signedReadParams(checked), context),
      "result",
    );
    const wireContext = record(wire["context"], "result.context");
    const value = record(wire["value"], "result.value");
    const cursor = readResponseCursor(value["cursor"], checked);
    const items = list(value["items"], "result.value.items");
    const skipped = list(value["skipped"], "result.value.skipped");
    if (items.length + skipped.length > Number(checked.limit ?? RING_READ_PAGE_LIMIT))
      throw invalid("result.value");
    return Object.freeze({
      slot: u64(wireContext["slot"], "result.context.slot"),
      blockTime: signedInteger(wireContext["blockTime"], "result.context.blockTime"),
      items: Object.freeze(
        items.map((item, index) => decodeTransaction(record(item, `result.value.items[${index}]`))),
      ),
      skipped: Object.freeze(
        skipped.map((entry, index) =>
          decodeSkipped(record(entry, `result.value.skipped[${index}]`)),
        ),
      ),
      ...defined({ cursor }),
    });
  }

  async readSignedWalletActivity(
    read: SignedRingRead,
    context?: RequestContext,
  ): Promise<RingWalletActivityPage> {
    const checked = checkedSignedRead(read, "getWalletActivity");
    const wire = record(
      await this.#call("getWalletActivity", signedReadParams(checked), context),
      "result",
    );
    const wireContext = record(wire["context"], "result.context");
    const value = record(wire["value"], "result.value");
    const cursor = readResponseCursor(value["cursor"], checked);
    const items = list(value["items"], "result.value.items");
    if (items.length > Number(checked.limit ?? RING_READ_PAGE_LIMIT)) throw invalid("result.value");
    return Object.freeze({
      slot: u64(wireContext["slot"], "result.context.slot"),
      blockTime: signedInteger(wireContext["blockTime"], "result.context.blockTime"),
      items: Object.freeze(
        items.map((item, index) =>
          decodeWalletActivity(record(item, `result.value.items[${index}]`)),
        ),
      ),
      ...defined({ cursor }),
    });
  }

  async getRelayQuote(request: RelayQuoteRequest, context?: RequestContext): Promise<RelayQuote> {
    return decodeRelayQuote(
      await this.#call(RingRpcMethod.getRelayQuote, encodeRelayQuoteRequest(request), context),
    );
  }

  async prepareRelay(request: PrepareRelayRequest, context?: RequestContext): Promise<RelayStep> {
    return decodeRelayStep(
      await this.#call(RingRpcMethod.prepareRelay, encodePrepareRelayRequest(request), context),
    );
  }

  /** Both salts as served, `privateRevocationKeys` opens each against its on-chain tag. */
  async getListSalt(
    request: GetListSaltRequest,
    context?: RequestContext,
  ): Promise<OwnerListSalts> {
    const wire = exactRecord(
      await this.#call(RingRpcMethod.getListSalt, encodeGetListSaltRequest(request), context),
      LIST_SALT_FIELDS,
      NO_FIELDS,
      "result",
    );
    const listSalt =
      wire["listSalt"] === undefined
        ? undefined
        : canonicalHash(wire["listSalt"], "result.listSalt");
    const ringSalt =
      wire["ringSalt"] === undefined
        ? undefined
        : canonicalHash(wire["ringSalt"], "result.ringSalt");
    return Object.freeze({
      ...defined({ listSalt, ringSalt }),
    });
  }

  async prepareAuthorityTransfer(
    request: PrepareAuthorityTransferRequest,
    context?: RequestContext,
  ): Promise<RelayStep> {
    return decodeRelayStep(
      await this.#call(
        RingRpcMethod.prepareAuthorityTransfer,
        encodePrepareAuthorityTransferRequest(request),
        context,
      ),
    );
  }

  async submitRelay(request: SubmitRelayRequest, context?: RequestContext): Promise<RelayStatus> {
    return decodeRelayStatus(
      await this.#call(RingRpcMethod.submitRelay, encodeSubmitRelayRequest(request), context),
    );
  }

  async getRelayStatus(
    relayId: RelayStep["relayId"],
    context?: RequestContext,
  ): Promise<RelayStatus> {
    return decodeRelayStatus(
      await this.#call(RingRpcMethod.getRelayStatus, encodeRelayStatusRequest(relayId), context),
    );
  }

  /** A request from `RingReaderGrantRequest.sign`, answered with the action the service now holds. */
  async setReaderGrant(
    request: SetReaderGrantRequest,
    context?: RequestContext,
  ): Promise<ReaderGrantAction> {
    return decodeReaderGrantAction(
      await this.#call(RingRpcMethod.setReaderGrant, encodeSetReaderGrantRequest(request), context),
    );
  }

  /** A request from `RingMergeSettingsRequest.sign`, answered with the settings the service stored. */
  async setMergeSettings(
    request: SetMergeSettingsRequest,
    context?: RequestContext,
  ): Promise<MergeSettings> {
    return decodeMergeSettings(
      await this.#call(
        RingRpcMethod.setMergeSettings,
        encodeSetMergeSettingsRequest(request),
        context,
      ),
    );
  }

  /** Ring RPC holds the write until its next `EnrollBatch`, the leaf enters a root only once that batch lands. */
  async submitEnrollment(
    request: SubmitEnrollmentRequest,
    context?: RequestContext,
  ): Promise<void> {
    decodeSubmitEnrollmentResponse(
      await this.#call(
        RingRpcMethod.submitEnrollment,
        encodeSubmitEnrollmentRequest(request),
        context,
      ),
    );
  }

  // `params` is the request object, not a positional list.
  async #walletSnapshotCall({
    input,
    method,
    context,
  }: Readonly<{
    input: RingWalletStatusQuery;
    method: "getWalletStatus" | "releaseWalletSnapshot";
    context: RequestContext | undefined;
  }>): Promise<unknown> {
    const target = checkedReadTarget(input.target);
    addressBytes(input.wallet, "wallet");
    const cursor = input.cursor === undefined ? undefined : checkedSnapshotCursor(input.cursor);
    const timestamp = timestampOrNow(input.timestamp);
    const nonce = freshNonce();
    const attestation = {
      target,
      wallet: input.wallet,
      timestamp,
      nonce,
      ...defined({ cursor }),
    };
    let message: Uint8Array;
    if (method === "getWalletStatus") {
      message = walletStatusAttestation(attestation);
    } else {
      if (cursor === undefined) throw invalidRequest("cursor");
      message = walletSnapshotReleaseAttestation({ ...attestation, cursor });
    }
    const signature = await input.signer.sign(message);
    return this.#call(
      method,
      signedReadParams({
        ringProgramId: target.ringProgramId,
        wallet: input.wallet,
        reader: input.signer.reader,
        timestamp,
        nonce,
        signature,
        ...defined({ cursor }),
      }),
      context,
    );
  }

  async #delegateSnapshotCall({
    input,
    method,
    context,
  }: Readonly<{
    input: RingDelegateNotesQuery;
    method: "getDelegateNotes" | "releaseDelegateSnapshot";
    context: RequestContext | undefined;
  }>): Promise<unknown> {
    const target = checkedReadTarget(input.target);
    addressBytes(input.owner, "owner");
    const cursor = input.cursor === undefined ? undefined : checkedSnapshotCursor(input.cursor);
    const timestamp = timestampOrNow(input.timestamp);
    const nonce = freshNonce();
    const attestation = {
      target,
      delegate: input.delegate.address,
      owner: input.owner,
      timestamp,
      nonce,
      ...defined({ cursor }),
    };
    let message: Uint8Array;
    if (method === "getDelegateNotes") {
      message = delegateNotesAttestation(attestation);
    } else {
      if (cursor === undefined) throw invalidRequest("cursor");
      message = delegateSnapshotReleaseAttestation({ ...attestation, cursor });
    }
    const signature = await signMessage(input.delegate, message);
    return this.#call(
      method,
      {
        ring: target.ringProgramId,
        delegate: input.delegate.address,
        owner: input.owner,
        timestamp: Number(timestamp),
        nonce: base64Decoder.decode(nonce),
        signature: base58Decoder.decode(signature),
        ...(cursor === undefined ? {} : { cursor: base64Decoder.decode(cursor) }),
      },
      context,
    );
  }

  async #call(
    method: string,
    params: Readonly<Record<string, unknown>> | undefined,
    context: RequestContext | undefined,
  ): Promise<unknown> {
    try {
      return await postJsonRpc(
        {
          fetch: this.#fetch,
          url: serviceUrl(this.#url, method, this.#apiKey),
          rpcMethod: method,
          params,
          id: 1,
          maxRequestBytes: RING_RPC_MAX_REQUEST_BYTES,
          maxResponseBytes: RING_RPC_MAX_RESPONSE_BYTES,
        },
        { timeoutMs: RING_RPC_TIMEOUT_MS, ...context },
      );
    } catch (error) {
      if (!(error instanceof TransportFailure)) throw error;
      const refusal = error.kind === "rpc" ? ringRpcRefusalName(error.facts["rpcCode"]) : undefined;
      if (refusal !== undefined)
        throw new RingError("RING_RPC_REFUSED", {
          details: { method, refusal, ...error.facts },
        });
      const status = error.kind === "http" ? error.facts["status"] : undefined;
      // Ring RPC answers -32250 to -32253 with 429 and -32254 with 401.
      if (status === 429)
        throw new RingError("RING_RPC_RATE_LIMITED", {
          details: { method, ...error.facts },
        });
      if (status === 401)
        throw new RingError("RING_RPC_CONFIG", {
          details: { method, field: "apiKey", ...error.facts },
        });
      const code =
        error.kind === "envelope" || error.kind === "missingResult" || error.kind === "rpc"
          ? "RING_RPC"
          : error.kind === "context" || error.kind === "config"
            ? "RING_RPC_CONFIG"
            : "RING_RPC_TRANSPORT";
      throw new RingError(code, { details: { method, ...error.facts } });
    }
  }
}
const LIST_SALT_FIELDS = new Set(["listSalt", "ringSalt"]);
const NO_FIELDS = new Set<string>();
