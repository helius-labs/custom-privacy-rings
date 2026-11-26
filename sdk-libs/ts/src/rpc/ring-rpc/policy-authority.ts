import type { MessagePartialSigner } from "@solana/kit";
import type { Address, Bytes32, Signature } from "@heliuslabs/spr/interface";
import { indexerScalars, type indexerTypes } from "@heliuslabs/spr/internal";
import { decodeCoSignListEntry } from "../../protocol/ring-rpc/ring-rpc-codec.js";
import type {
  PolicyAuthorityAuth,
  CoSignList,
  CoSignListEdit,
  CoSignListEntry,
  CoSignSubjectKind,
  CreateRecordsRegistryRequest,
  EditCoSignListRequest,
  GetCoSignListsRequest,
  GetPendingListApprovalsRequest,
  RefuseListApprovalRequest,
  RegisterEnrollmentRingRequest,
} from "../../protocol/ring-rpc/ring-rpc-types.js";
import {
  type Hash,
  encoder,
  base64Decoder,
  address,
  checkedUnixTimestamp,
  base58Decoder,
} from "./wire.js";
import {
  type RingReadTarget,
  targetLines,
  checkedReadTarget,
  timestampOrNow,
  freshNonce,
} from "./signed-request.js";
import { signMessage } from "../../protocol/ring-rpc/read-signer.js";

const POLICY_AUTHORITY_DOMAIN = "zolana/ring-rpc-policy-authority";

/** Rust `ListPendingApprovals`, `RefuseListApproval`, `CreateRecords` or `RegisterEnrollment`, the action the policy authority signs. */
export type PolicyAuthorityAction =
  | Readonly<{ kind: "list"; registry: Address; cursor?: Hash }>
  | Readonly<{ kind: "refuse"; registry: Address; updateId: Hash }>
  | Readonly<{ kind: "createRecords" }>
  | Readonly<{ kind: "registerEnrollment"; writer: Address }>
  /** Rust `EditCoSignList`. */
  | Readonly<{
      kind: "editCoSignList";
      edit: CoSignListEdit;
      list: CoSignList;
      subjectKind: CoSignSubjectKind;
      address: Address;
    }>
  /** Rust `ReadCoSignLists`. */
  | Readonly<{ kind: "readCoSignLists"; cursor?: CoSignListEntry }>;

export type ListApprovalTarget = RingReadTarget & Readonly<{ registry: Address }>;

/** Mirrors Rust `PolicyAuthorityAttestation::message`. */
export function policyAuthorityAttestation(
  input: Readonly<{
    target: RingReadTarget;
    authority: Address;
    action: PolicyAuthorityAction;
    timestamp: bigint;
    nonce: Bytes32;
  }>,
): Uint8Array {
  const action = policyAuthorityActionLine(input.action);
  return encoder.encode(
    [
      POLICY_AUTHORITY_DOMAIN,
      ...targetLines(input.target),
      `authority: ${input.authority}`,
      `action: ${action}`,
      `timestamp: ${input.timestamp}`,
      `nonce: ${base64Decoder.decode(input.nonce)}`,
    ].join("\n"),
  );
}

function policyAuthorityActionLine(action: PolicyAuthorityAction): string {
  switch (action.kind) {
    case "refuse":
      return `refuse ${address(action.registry, "registry")} ${indexerScalars.checkedHash(action.updateId, "updateId")}`;
    case "createRecords":
      return "create records";
    case "registerEnrollment":
      return `register enrollments by ${address(action.writer, "writer")}`;
    case "list":
      return action.cursor === undefined
        ? `list ${address(action.registry, "registry")}`
        : `list ${address(action.registry, "registry")} after ${indexerScalars.checkedHash(action.cursor, "cursor")}`;
    case "editCoSignList":
      return `${action.edit} co-sign ${action.list} ${action.subjectKind} ${address(action.address, "address")}`;
    case "readCoSignLists":
      return action.cursor === undefined
        ? "read co-sign lists"
        : `read co-sign lists after ${action.cursor.list} ${action.cursor.kind} ${indexerScalars.checkedHash(action.cursor.subject, "cursor.subject")}`;
  }
}

/** Rust `GetPendingListApprovalsRequest::read` signed by the policy authority, the page after `cursor`. */
export async function signPendingListApprovalsRequest(
  target: ListApprovalTarget,
  authority: MessagePartialSigner,
  cursor?: Hash,
  timestamp?: bigint,
): Promise<GetPendingListApprovalsRequest> {
  const checked = checkedReadTarget(target);
  const registry = address(target.registry, "registry");
  const action = { kind: "list" as const, registry, ...(cursor === undefined ? {} : { cursor }) };
  return Object.freeze({
    ring: checked.ringProgramId,
    registry,
    ...(cursor === undefined ? {} : { cursor }),
    auth: await policyAuthorityAuth(checked, authority, action, timestamp),
  });
}

/** Rust `RefuseListApprovalRequest::refusal` signed by the policy authority. */
export async function signRefuseListApprovalRequest(
  target: ListApprovalTarget,
  authority: MessagePartialSigner,
  updateId: Hash,
  timestamp?: bigint,
): Promise<RefuseListApprovalRequest> {
  const checked = checkedReadTarget(target);
  const registry = address(target.registry, "registry");
  return Object.freeze({
    ring: checked.ringProgramId,
    registry,
    updateId,
    auth: await policyAuthorityAuth(
      checked,
      authority,
      { kind: "refuse", registry, updateId },
      timestamp,
    ),
  });
}

/** Rust `CreateRecordsRegistryRequest::create` signed by the policy authority, or the upgrade authority before `Initialize`. */
export async function signCreateRecordsRegistryRequest(
  target: RingReadTarget,
  authority: MessagePartialSigner,
  timestamp?: bigint,
): Promise<CreateRecordsRegistryRequest> {
  const checked = checkedReadTarget(target);
  return Object.freeze({
    ring: checked.ringProgramId,
    auth: await policyAuthorityAuth(checked, authority, { kind: "createRecords" }, timestamp),
  });
}

/** Rust `RegisterEnrollmentRingRequest::register` signed by the policy authority, `writer` is the `enrollmentWriterPublicKey` of the service's health. */
export async function signRegisterEnrollmentRingRequest(
  target: RingReadTarget,
  authority: MessagePartialSigner,
  writer: Address,
  timestamp?: bigint,
): Promise<RegisterEnrollmentRingRequest> {
  const checked = checkedReadTarget(target);
  return Object.freeze({
    ring: checked.ringProgramId,
    auth: await policyAuthorityAuth(
      checked,
      authority,
      { kind: "registerEnrollment", writer: address(writer, "writer") },
      timestamp,
    ),
  });
}

/** Rust `EditCoSignListRequest::edit` signed by the policy authority. */
export async function signEditCoSignListRequest(
  target: RingReadTarget,
  authority: MessagePartialSigner,
  edit: Pick<EditCoSignListRequest, "edit" | "list" | "kind" | "address">,
  timestamp?: bigint,
): Promise<EditCoSignListRequest> {
  const checked = checkedReadTarget(target);
  const subject = address(edit.address, "address");
  return Object.freeze({
    ring: checked.ringProgramId,
    edit: edit.edit,
    list: edit.list,
    kind: edit.kind,
    address: subject,
    auth: await policyAuthorityAuth(
      checked,
      authority,
      {
        kind: "editCoSignList",
        edit: edit.edit,
        list: edit.list,
        subjectKind: edit.kind,
        address: subject,
      },
      timestamp,
    ),
  });
}

/** Rust `GetCoSignListsRequest::read` signed by the policy authority, the page after `cursor`. */
export async function signCoSignListsRequest(
  target: RingReadTarget,
  authority: MessagePartialSigner,
  cursor?: CoSignListEntry,
  timestamp?: bigint,
): Promise<GetCoSignListsRequest> {
  const checked = checkedReadTarget(target);
  const page = cursor === undefined ? {} : { cursor: decodeCoSignListEntry(cursor) };
  return Object.freeze({
    ring: checked.ringProgramId,
    ...page,
    auth: await policyAuthorityAuth(
      checked,
      authority,
      { kind: "readCoSignLists", ...page },
      timestamp,
    ),
  });
}

async function policyAuthorityAuth(
  target: RingReadTarget,
  authority: MessagePartialSigner,
  action: PolicyAuthorityAction,
  timestamp: bigint | undefined,
): Promise<PolicyAuthorityAuth> {
  const signedAt = timestampOrNow(
    timestamp === undefined ? undefined : checkedUnixTimestamp(timestamp, "timestamp"),
  );
  const nonce = freshNonce();
  const signature = await signMessage(
    authority,
    policyAuthorityAttestation({
      target,
      authority: authority.address,
      action,
      timestamp: signedAt,
      nonce,
    }),
  );
  return Object.freeze({
    authority: authority.address,
    timestamp: signedAt,
    nonce: base64Decoder.decode(nonce) as indexerTypes.Base64String,
    signature: base58Decoder.decode(signature) as Signature,
  });
}
