import { getBase58Decoder, getBase64Decoder, type Instruction } from "@solana/kit";

import type { ProofDataSourceContext, TreeContext, WalletKeys } from "@heliuslabs/spr/client";
import type { LatestBlockhash } from "@heliuslabs/spr/internal";
import {
  addressBytes,
  bigIntBytes,
  prepareProverMerge,
  compressG2,
  withProofDataRetry,
  compileUnsignedTransaction,
  prepareMerge,
  reserveEntries,
  reservedUtxoKeys,
  unreserved,
  encodeMergeTransactInstructionData,
  mergeMintChunks,
  mergePaddedInputCount,
  SYSTEM_PROGRAM,
  instruction,
  meta,
  solanaOwnerIdentity,
  tagged,
  type SignerAccount,
  nullifierPdaAddress,
  ZERO_32,
  ZERO_NULLIFIER_PK,
  checkIntentApproval,
  ownerSolanaAccount,
  equalBytes,
  type UtxoReservation,
} from "@heliuslabs/spr/internal";
import { initializePoseidon } from "@heliuslabs/spr";
import {
  MAX_MERGE_INPUTS,
  MERGE_INPUT_COUNT,
  SHIELDED_POOL_PROGRAM_ID,
} from "@heliuslabs/spr/interface";
import type {
  Address,
  Bytes32,
  Bytes33,
  Bytes64,
  Bytes128,
  MergeTransactInstructionData,
  RequestContext,
  Signature,
  Transaction,
} from "@heliuslabs/spr/interface";
import type { NullifierKey, ShieldedAddress } from "@heliuslabs/spr/keypair";
import {
  assetField,
  SOL_MINT,
  approveUnattended,
  intentHash,
  type ApprovalHandler,
  type TransactionIntent,
  checkKeysIdentity,
} from "@heliuslabs/spr/transaction";
import type { PreparedMerge, Wallet, WalletUtxo } from "@heliuslabs/spr/transaction";
import {
  encodeRelayMergeInputs,
  mergeRequestDigest,
} from "../../protocol/ring-rpc/relay-proof-inputs.js";
import type { RelayIntent, RelayQuote } from "../../protocol/ring-rpc/ring-rpc-types.js";
import { PreparedRelay } from "../../rpc/relay.js";
import type { RingMergeClient, TrackingReader } from "../../rpc/ports.js";
import { relayRequestAttestation, type RingServiceTarget } from "../../rpc/ring-rpc.js";
import { ringAuthAddress, ringPolicyAddress } from "../../protocol/pda/index.js";
import { CUSTOM_RING_PROOF_LENGTH } from "../../protocol/codec/ring-codecs.js";
import {
  MERGE_ENVELOPE_LEN,
  MERGE_RECORD_LEN,
} from "../../protocol/codec/ring-disclosure-layout.js";
import { ringIdField } from "../../crypto/derivation/ring-stream.js";

import {
  checkAnonymousSigner,
  checkHiddenParties,
  type AnonymousSpendSigner,
} from "./anonymous.js";
import { fetchRingPolicy } from "../../protocol/membership/config.js";
import { isZeroKeyNote } from "../../state/exposed-notes.js";
import { RingError, wrapRingError } from "../../protocol/error.js";
import { RingInstructionTag } from "../../protocol/transaction/instruction-tag.js";
import {
  compactMergeBody,
  encodeRingMergeRequest,
  type RingMergeDisclosureRequest,
  type RingMergeSealer,
  type SealedMergeRecord,
} from "../../crypto/audit/merge-disclosure.js";
import { newestTrackingRoot } from "../../rpc/tracking-resolver.js";
import type { TrackingOpening } from "../../protocol/membership/tracking.js";
import {
  RingTransactionSubmission,
  checkRetainedEntries,
  type RingSubmissionAttempt,
  type RingSubmissionBuildState,
} from "../workflow/submission.js";
import {
  noteNullifierPublicKey,
  ringIntentMismatch,
  ringProofInput,
  ringSelectionErrors,
  selectCoveredUtxos,
} from "../workflow/selection.js";
import { MERGE_ROOT_READ, readOutputKeys, ringRegistrySlots } from "../workflow/ring-reads.js";
import { resolveRingOutputTree } from "../../protocol/membership/trees.js";
import { fitsOneTransaction, SIZE_PROBE_LIFETIME, transactionSize } from "../workflow/compile.js";

const base64 = getBase64Decoder();

/** Rust `MAX_RING_MERGE_INPUTS`, one ring merge key serves every SPP width. */
export const MAX_RING_MERGE_INPUTS = MAX_MERGE_INPUTS;
/** Rust `compute_unit_limit(Merge)`. */
export const RING_MERGE_COMPUTE_UNIT_LIMIT = 1_400_000;
/** `expiry_unix_ts u64 ‖ proof.a` ahead of `proof.b` in the SPP merge body. */
const MERGE_B_OFFSET = 8 + 32;

export interface RingMergeTransactionParams {
  readonly client: RingMergeClient &
    ProofDataSourceContext &
    Pick<TrackingReader, "getTrackingOpenings">;
  readonly ringProgramId: Address;
  readonly wallet: Wallet;
  /** The seed reaches the record only through `sealRingMerge`. */
  readonly keys: WalletKeys & Partial<RingMergeSealer>;
  readonly feePayer: Address;
  readonly asset?: Address;
  /** The most notes merged, smallest first, from 2 through `MAX_RING_MERGE_INPUTS`, default `MERGE_INPUT_COUNT`. */
  readonly maxInputs?: number;
  readonly outputTree?: Address;
  readonly approve?: ApprovalHandler;
}

export interface RingMergeRelayParams extends Pick<
  RingMergeTransactionParams,
  "ringProgramId" | "wallet" | "asset" | "maxInputs" | "approve"
> {
  readonly client: TreeContext;
  /** Ring RPC proves with it and so links every spend of the owner. */
  readonly nullifierKey: NullifierKey;
  /** The owner's key, it signs the request text, Ring RPC proves nothing for an unsigned request. */
  readonly signer: AnonymousSpendSigner;
  /** The Ring RPC the owner's request text names, its service key pinned by the caller. */
  readonly ringRpc: RingServiceTarget;
  readonly quote: RelayQuote;
}

/** The selected notes stay leased for the reservation lifetime. */
export async function prepareRingMergeRelay(params: RingMergeRelayParams): Promise<PreparedRelay> {
  let reservation: UtxoReservation | undefined;
  let payload: Uint8Array | undefined;
  try {
    await initializePoseidon();
    const sender = params.wallet.identity;
    checkMergeOwnerKey(sender);
    const { feePayer } = params.quote;
    const owner = checkAnonymousSigner({ signer: params.signer, sender });
    // The merge circuit admits a hidden owner as payer, the wallet keeps one off it.
    checkHiddenParties(owner, [feePayer]);
    const asset = params.asset ?? SOL_MINT;
    const notes = selectMergeNotes({
      wallet: params.wallet,
      ringProgramId: params.ringProgramId,
      tree: params.client.tree,
      treeId: params.client.treeId,
      asset,
      maxInputs: mergeWidth(params.maxInputs),
    });
    reservation = reserveEntries(params.wallet, notes);
    await approveMerge({
      approve: params.approve,
      owner: sender,
      feePayer,
      intent: {
        kind: "ringMerge",
        ringProgramId: params.ringProgramId,
        outputTree: params.client.tree,
        asset,
        numInputs: notes.length,
        mergedAmount: notes.reduce((total, note) => total + note.utxo.amount, 0n),
      },
    });
    payload = encodeRelayMergeInputs({
      ringProgramId: params.ringProgramId,
      sender,
      nullifierKey: params.nullifierKey,
      inputs: notes.map((note) => ringProofInput(note, sender, params.client)),
      leafIndexes: notes.map((note) => note.outputContext.leafIndex),
      assets: params.wallet.registry,
    });
    const request = relayRequestAttestation({
      target: { ...params.ringRpc, ringProgramId: params.ringProgramId },
      signer: getBase58Decoder().decode(owner) as Address,
      request: { kind: "merge", digest: mergeRequestDigest(payload) },
    });
    const signature = await params.signer.sign(request);
    if (!(signature instanceof Uint8Array) || signature.length !== 64)
      throw new RingError("RING_RELAY_OWNER_SIGNATURE", {
        details: { length: signature instanceof Uint8Array ? signature.length : -1 },
      });
    const ownerSignature = getBase58Decoder().decode(signature) as Signature;
    return new PreparedRelay({
      ring: params.ringProgramId,
      quote: params.quote,
      intent: {
        operation: "merge",
        proofInputs: base64.decode(payload) as RelayIntent["proofInputs"],
        ownerSignature,
      },
    });
  } catch (cause) {
    if (reservation !== undefined) params.wallet._releaseReservation(reservation.id);
    throw wrapRingError("RING_BUILD_MERGE", cause);
  } finally {
    payload?.fill(0);
  }
}

export async function buildRingMergeTransaction(
  params: RingMergeTransactionParams,
  context?: RequestContext,
): Promise<Transaction> {
  return (await buildRingMerge(params, {}, context)).transaction;
}

/** A rebuild proves the same notes again under the registry's newest root. */
export function createRingMergeSubmission(
  params: RingMergeTransactionParams,
  context?: RequestContext,
): Promise<RingTransactionSubmission> {
  return RingTransactionSubmission.fromBuilder(
    {
      wallet: params.wallet,
      build: (retry, context) => buildRingMerge(params, retry, context),
      registry: params.client,
    },
    context,
  );
}

/** Tag 57, the accounts `[ring, tracking]` ahead of SPP's `merge_ring` list. */
export async function ringMergeInstruction(
  input: Readonly<{
    ringProgramId: Address;
    inputTree: Address;
    outputTree: Address;
    payer: SignerAccount;
    data: MergeTransactInstructionData;
    outputRingDataHash: Bytes32;
    sealed: SealedMergeRecord;
    /** The ring's pinned registry. */
    tracking: Address;
    trackingRevision: bigint;
  }>,
): Promise<Instruction> {
  const [ring, auth, nullifiers] = await Promise.all([
    ringPolicyAddress(input.ringProgramId),
    ringAuthAddress(input.ringProgramId),
    Promise.all(input.data.nullifiers.map((value) => nullifierPdaAddress(input.inputTree, value))),
  ]);
  const body = encodeMergeTransactInstructionData(input.data);
  return instruction(
    tagged(
      RingInstructionTag.merge,
      encodeRingMergeRequest({
        trackingRevision: input.trackingRevision,
        outputRingDataHash: input.outputRingDataHash,
        sealed: input.sealed,
        compactBody: compactMergeBody(
          body,
          compressG2(body.slice(MERGE_B_OFFSET, MERGE_B_OFFSET + 128) as Bytes128) as Bytes64,
        ),
      }),
    ),
    [
      meta(ring, false, false),
      meta(input.tracking, false, false),
      meta(input.inputTree, false, true),
      meta(input.outputTree, false, true),
      meta(auth, false, false),
      meta(input.payer, true, true),
      meta(SYSTEM_PROGRAM, false, false),
      meta(SHIELDED_POOL_PROGRAM_ID, false, false),
      ...nullifiers.map((key) => meta(key, false, true)),
    ],
    input.ringProgramId,
  );
}

async function buildRingMerge(
  params: RingMergeTransactionParams,
  retry: RingSubmissionBuildState,
  context?: RequestContext,
): Promise<RingSubmissionAttempt> {
  try {
    await initializePoseidon();
    checkKeysIdentity(params.keys, params.wallet.identity);
    const maxInputs = mergeWidth(params.maxInputs);
    const owner = params.keys.address();
    checkMergeOwnerKey(owner);
    const [config, outputTree] = await Promise.all([
      fetchRingPolicy(params.client, params.ringProgramId, context),
      resolveRingOutputTree(params.client, params.outputTree, context),
    ]);
    // A hidden owner stays off the payer.
    const ownerKey = owner.signingPublicKey.confidentialViewTag() as Bytes32;
    checkHiddenParties(ownerKey, [params.feePayer]);
    const asset = params.asset ?? SOL_MINT;
    if (retry.entries !== undefined) checkRetainedEntries(params.wallet, retry.entries);
    const entries =
      retry.entries ??
      selectMergeNotes({
        wallet: params.wallet,
        ringProgramId: params.ringProgramId,
        tree: params.client.tree,
        treeId: params.client.treeId,
        asset,
        maxInputs,
      });
    if (mergePaddedInputCount(entries.length) === undefined)
      throw new RingError("RING_TOO_MANY_INPUTS", {
        details: { selected: entries.length, maximum: MAX_RING_MERGE_INPUTS },
      });
    retry.entries = entries;
    retry.reservation ??= reserveEntries(params.wallet, entries, retry.lifetime);
    const ownerPkHash = owner.signingPublicKey.ownerProofInputHash();
    const trackingRoot = await newestTrackingRoot(
      {
        client: params.client,
        trackingRegistry: config.tracking,
        ring: params.ringProgramId,
      },
      context,
    );
    const [registry] = ringRegistrySlots(config, MERGE_ROOT_READ);
    if (registry === undefined) throw new RingError("RING_TRACKING_INVALID");
    const mergeInstruction = (
      data: MergeTransactInstructionData,
      sealed: SealedMergeRecord,
    ): Promise<Instruction> =>
      ringMergeInstruction({
        ringProgramId: params.ringProgramId,
        inputTree: params.client.tree,
        outputTree: outputTree.tree,
        payer: params.feePayer,
        data,
        outputRingDataHash: ZERO_32,
        sealed,
        tracking: registry,
        trackingRevision: trackingRoot.revision,
      });
    const compile = async (
      data: MergeTransactInstructionData,
      lifetime: LatestBlockhash,
      sealed: SealedMergeRecord,
    ): Promise<Transaction> =>
      compileUnsignedTransaction({
        feePayer: params.feePayer,
        lifetime,
        instructions: [await mergeInstruction(data, sealed)],
        computeUnitLimit: RING_MERGE_COMPUTE_UNIT_LIMIT,
      });
    const selected = entries.map((entry) => ringProofInput(entry, owner, params.client));
    const prepareCount = (count: number): Promise<PreparedMerge> =>
      prepareMerge(
        {
          keys: params.keys,
          inputs: selected.slice(0, count),
          outputTreeId: outputTree.treeId,
          ring: { programId: params.ringProgramId },
          invalidAnswers: () => ringIntentMismatch("keyAnswers"),
        },
        context,
      );
    // Rust `FittedMerge`, the longest prefix one transaction carries.
    const { count, value: prepared } = await longestFitting({
      count: entries.length,
      minimum: MIN_MERGE_INPUTS,
      prepare: prepareCount,
      fits: async (candidate) =>
        fitsOneTransaction(
          transactionSize({
            feePayer: params.feePayer,
            instructions: [
              await mergeInstruction(
                sizingData(candidate, params.client.tree),
                EMPTY_SEALED_RECORD,
              ),
            ],
            computeUnitLimit: RING_MERGE_COMPUTE_UNIT_LIMIT,
          }),
        ),
    });
    // The minimum passes untested, a width past the limits refuses here.
    await compile(
      sizingData(prepared, params.client.tree),
      SIZE_PROBE_LIFETIME,
      EMPTY_SEALED_RECORD,
    );
    const proofInputs = prepareProverMerge(prepared, params.client.tree);
    const inputs = selected.slice(0, count);
    if (count < entries.length) {
      params.wallet._releaseReservation(retry.reservation.id);
      retry.entries = entries.slice(0, count);
      retry.reservation = reserveEntries(params.wallet, retry.entries, retry.lifetime);
    }
    const expectedHash = prepared.outputHash();
    const [tracking] = await readOutputKeys(
      {
        client: params.client,
        root: trackingRoot,
        outputs: [{ member: ownerPkHash, nullifierPk: prepared.nullifierPublicKey }],
      },
      context,
    );
    if (tracking === undefined) throw new RingError("RING_TRACKING_INVALID");
    const intent: RingMergeIntent = {
      kind: "ringMerge",
      ringProgramId: params.ringProgramId,
      outputTree: outputTree.tree,
      asset,
      numInputs: inputs.length,
      mergedAmount: prepared.output.amount,
    };
    const planned = intentHash(intent);
    if (retry.intent !== undefined && !equalBytes(retry.intent, planned))
      throw ringIntentMismatch("retryIntent");
    retry.intent = planned;
    await approveMerge({ approve: params.approve, owner, feePayer: params.feePayer, intent });
    const { data } = await withProofDataRetry(
      params.client.proofDataSource,
      (attempt) => params.client.proveMerge({ prepared, keys: params.keys }, attempt),
      context,
    );
    if (!equalBytes(data.outputUtxoHash, expectedHash)) throw ringIntentMismatch("output");
    const request = mergeDisclosureRequest({
      prepared,
      spp: data,
      ringProgramId: params.ringProgramId,
      auditorU: config.auditorPublicKey.u(),
      recipientU: owner.viewingPublicKey.u(),
      payer: params.feePayer,
      ownerPkHash,
      outputTreeId: bigIntBytes(BigInt(proofInputs.inputs.payload.outputTreeId)) as Bytes32,
      trackingRoot: trackingRoot.root,
      tracking,
    });
    const sealed = await sealUnderHolder(params.keys, request, context);
    const lifetime = await params.client.getLatestBlockhash(context);
    return {
      transaction: await compile(data, lifetime, sealed),
      lastValidBlockHeight: lifetime.lastValidBlockHeight,
      intentHash: planned,
      ringInstructionIndex: 0,
      trackingRoot,
    };
  } catch (cause) {
    if (retry.reservation !== undefined) params.wallet._releaseReservation(retry.reservation.id);
    throw wrapRingError("RING_BUILD_MERGE", cause);
  }
}

type RingMergeIntent = Extract<TransactionIntent, { kind: "ringMerge" }>;

const MIN_MERGE_INPUTS = 2;

/** Zeroed proofs over the real accounts size the transaction before any proof request. */
function sizingData(prepared: PreparedMerge, tree: Address): MergeTransactInstructionData {
  return prepareProverMerge(prepared, tree)
    .finish({
      slot: { id: prepared.inputTreeId, utxoRoot: ZERO_32, nullifierRoot: ZERO_32 },
      utxoRootIndex: 0,
      nullifierRootIndex: 0,
    })
    .instructionData({ a: ZERO_32, b: new Uint8Array(128) as Bytes128, c: ZERO_32 });
}

/** @internal The minimum is taken unmeasured. */
export async function longestFitting<T>(
  input: Readonly<{
    count: number;
    minimum: number;
    prepare: (count: number) => Promise<T>;
    fits: (value: T) => Promise<boolean>;
  }>,
): Promise<Readonly<{ count: number; value: T }>> {
  let { count } = input;
  let value = await input.prepare(count);
  while (count > input.minimum && !(await input.fits(value))) {
    count -= 1;
    value = await input.prepare(count);
  }
  return { count, value };
}

const EMPTY_SEALED_RECORD: SealedMergeRecord = Object.freeze({
  proof: new Uint8Array(CUSTOM_RING_PROOF_LENGTH),
  ephemeralPublicKey: new Uint8Array(33) as Bytes33,
  envelope: new Uint8Array(MERGE_ENVELOPE_LEN),
  record: new Uint8Array(MERGE_RECORD_LEN),
});

/** The statement binds the proved SPP merge `spp`. */
function mergeDisclosureRequest(
  input: Readonly<{
    prepared: PreparedMerge;
    spp: MergeTransactInstructionData;
    ringProgramId: Address;
    auditorU: Bytes32;
    /** `V`, the merged owner's viewing key the record seals to. */
    recipientU: Bytes32;
    payer: Address;
    ownerPkHash: Bytes32;
    outputTreeId: Bytes32;
    trackingRoot: Bytes32;
    tracking: TrackingOpening;
  }>,
): RingMergeDisclosureRequest {
  const { prepared, spp } = input;
  const firstNullifier = spp.nullifiers[0];
  if (firstNullifier === undefined) throw ringIntentMismatch("firstNullifier");
  const mint = prepared.output.asset;
  return Object.freeze({
    kind: "merge" as const,
    outputUtxoHash: spp.outputUtxoHash,
    firstNullifier,
    ringProgramId: ringIdField(input.ringProgramId),
    auditorPk: input.auditorU,
    recipientKey: input.recipientU,
    payer: solanaOwnerIdentity(addressBytes(input.payer)) as Bytes32,
    trackingRoot: input.trackingRoot,
    // The merge proves no owner signature, a PDA owner key is valid.
    ownerKey: prepared.signingPublicKey.confidentialViewTag() as Bytes32,
    ownerPkHash: input.ownerPkHash,
    mint,
    mintChunks: mergeMintChunks(mint).map((chunk) => bigIntBytes(chunk) as Bytes32),
    asset: assetField(mint),
    amount: bigIntBytes(prepared.output.amount) as Bytes32,
    nullifierPk: prepared.nullifierPublicKey,
    outputTreeId: input.outputTreeId,
    outputRingDataHash: ZERO_32,
    outputEncryption: spp.outputEncryption,
    tracking: input.tracking,
  });
}

async function sealUnderHolder(
  keys: Partial<RingMergeSealer>,
  request: RingMergeDisclosureRequest,
  context: RequestContext | undefined,
): Promise<SealedMergeRecord> {
  const seal = keys.sealRingMerge?.bind(keys);
  if (seal === undefined) throw new RingError("RING_KEYS_REQUIRED");
  return seal(request, context);
}

function selectMergeNotes(
  input: Readonly<{
    wallet: Wallet;
    ringProgramId: Address;
    tree: Address;
    treeId: number;
    asset: Address;
    maxInputs: number;
  }>,
): readonly WalletUtxo[] {
  const reserved = reservedUtxoKeys(input.wallet);
  const owner = input.wallet.identity;
  return selectCoveredUtxos({
    wallet: input.wallet,
    asset: input.asset,
    target: { kind: "consolidate", minInputs: 2 },
    policy: {
      // One merge spends one nullifier key.
      eligible: (entry) =>
        unreserved(reserved)(entry) &&
        entry.utxo.ringProgramId === input.ringProgramId &&
        entry.utxo.owner.equals(owner.signingPublicKey) &&
        !isZeroKeyNote(entry) &&
        equalBytes(noteNullifierPublicKey(entry, owner, input.treeId), owner.nullifierPublicKey) &&
        entry.dataHash === undefined &&
        entry.utxo.data.records().every((record) => record.kind !== "utxoData"),
      ordering: "smallestFirst",
      maxInputs: input.maxInputs,
      tree: { kind: "fixed", tree: input.tree },
      errors: {
        ...ringSelectionErrors,
        tooFewUtxos: () => new RingError("RING_NOTHING_TO_MERGE"),
      },
    },
  }).entries;
}

async function approveMerge(
  input: Readonly<{
    approve: ApprovalHandler | undefined;
    owner: ShieldedAddress;
    feePayer: Address;
    intent: RingMergeIntent;
  }>,
): Promise<void> {
  const { intent } = input;
  const approval = await (input.approve ?? approveUnattended)({
    solanaPublicKey: ownerSolanaAccount(input.owner, input.feePayer),
    intent,
    summary: `merge ${String(intent.numInputs)} inputs in ring ${intent.ringProgramId}`,
  });
  checkIntentApproval(approval, intent, ringIntentMismatch);
}

/** The key's secret seeds every derivation, a zero secret would make each one public, Rust `MergeOwnerKey`. */
function checkMergeOwnerKey(owner: ShieldedAddress): void {
  if (equalBytes(owner.nullifierPublicKey, ZERO_NULLIFIER_PK))
    throw new RingError("RING_ZERO_KEY_MERGE_UNAVAILABLE");
}

function mergeWidth(maxInputs = MERGE_INPUT_COUNT): number {
  if (maxInputs > MAX_RING_MERGE_INPUTS) {
    throw new RingError("RING_TOO_MANY_INPUTS", {
      details: { selected: maxInputs, maximum: MAX_RING_MERGE_INPUTS },
    });
  }
  if (!Number.isSafeInteger(maxInputs) || maxInputs < 2) {
    throw new RingError("RING_NOTHING_TO_MERGE");
  }
  return maxInputs;
}
