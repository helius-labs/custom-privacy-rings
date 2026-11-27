import { ClientError, type RequestContext } from "@heliuslabs/spr";
import {
  ApiError,
  equalBytes,
  wrapIndexer,
  type MethodDescriptor,
  type SprApi,
} from "@heliuslabs/spr/internal";

import {
  getStorageMemberOpeningsMethod,
  getStorageMemberRecordsMethod,
  getStorageRecordPoolMethod,
} from "../protocol/indexer/indexer-codec.js";
import { QUAD_ARITY, QUAD_HEIGHT, canopyLevels } from "../protocol/membership/member-registry.js";
import {
  getTrackingOpeningsMethod,
  getTrackingRecordPoolMethod,
  type TrackingOpenings,
  type TrackingOpeningsRequest,
  type TrackingRecordPoolRequest,
  type TrackingRecordPool,
} from "../protocol/indexer/tracking-codec.js";
import { TRACKING_QUAD_HEIGHT, type ServedTrackingPool } from "../protocol/membership/tracking.js";
import type {
  RingRecordReader,
  TrackingReader,
  StorageMemberOpenings,
  StorageMemberPage,
  StorageOpeningsRequest,
  StoragePageRequest,
  StorageRecordPool,
  StorageRecordPoolRequest,
} from "./ports.js";
import { RingError, type RingErrorCode } from "../protocol/error.js";

/** The ring indexer's refusal codes, Rust `zolana_ring_indexer::rpc::RingRpcError`. */
const RING_PROJECTION_CODES: ReadonlyMap<number, RingErrorCode> = new Map([
  [-32070, "RING_REGISTRY_OUT_OF_SYNC"],
  [-32071, "RING_REGISTRY_ROOT_CHANGED"],
  [-32075, "RING_REGISTRY_PAGE_GAP"],
]);

export class RingIndexer implements RingRecordReader, TrackingReader {
  readonly #api: SprApi;

  constructor(api: SprApi) {
    this.#api = api;
  }

  async getStorageMemberOpenings(
    request: StorageOpeningsRequest,
    context?: RequestContext,
  ): Promise<StorageMemberOpenings> {
    const method = "getStorageMemberOpenings";
    try {
      const response = await this.#api.call(getStorageMemberOpeningsMethod, request, context);
      if (
        !equalBytes(response.root.root, request.expectedRoot) ||
        response.openings.length !== request.members.length
      )
        throw new ClientError("CLIENT_INVALID_RPC_RESPONSE", { details: { method, path: "$" } });
      return response;
    } catch (cause) {
      throw ringIndexerError(cause, method);
    }
  }

  async getTrackingOpenings(
    request: TrackingOpeningsRequest,
    context?: RequestContext,
  ): Promise<TrackingOpenings> {
    const method = "getTrackingOpenings";
    try {
      const response = await this.#api.call(getTrackingOpeningsMethod, request, context);
      if (
        !equalBytes(response.root, request.expectedRoot) ||
        response.openings.length !== request.members.length
      )
        throw new ClientError("CLIENT_INVALID_RPC_RESPONSE", { details: { method, path: "$" } });
      return response;
    } catch (cause) {
      throw ringIndexerError(cause, method);
    }
  }

  async getTrackingRecordPool(
    request: TrackingRecordPoolRequest,
    context?: RequestContext,
  ): Promise<ServedTrackingPool> {
    return this.#recordPool(getTrackingRecordPoolMethod, request, TRACKING_QUAD_HEIGHT, context);
  }

  async getStorageMemberRecords(
    request: StoragePageRequest,
    context?: RequestContext,
  ): Promise<StorageMemberPage> {
    const method = "getStorageMemberRecords";
    try {
      const response = await this.#api.call(getStorageMemberRecordsMethod, request, context);
      if (!equalBytes(response.root.root, request.expectedRoot))
        throw new ClientError("CLIENT_INVALID_RPC_RESPONSE", {
          details: { method, path: "$.root" },
        });
      return response;
    } catch (cause) {
      throw ringIndexerError(cause, method);
    }
  }

  async getStorageRecordPool(
    request: StorageRecordPoolRequest,
    context?: RequestContext,
  ): Promise<StorageRecordPool> {
    return this.#recordPool(getStorageRecordPoolMethod, request, QUAD_HEIGHT, context);
  }

  async #recordPool<T extends StorageRecordPool | TrackingRecordPool>(
    method: MethodDescriptor<StorageRecordPoolRequest, T>,
    request: StorageRecordPoolRequest,
    height: number,
    context: RequestContext | undefined,
  ): Promise<T> {
    try {
      const response = await this.#api.call(method, request, context);
      const levels = canopyLevels(request.openings, height);
      if (
        !equalBytes(response.root.root, request.expectedRoot) ||
        response.records.length < 1 ||
        response.records.length > request.openings ||
        response.canopy.length !== QUAD_ARITY ** levels ||
        response.subjects.length !== request.members.length ||
        response.subjects.some((index) => index >= response.records.length) ||
        response.records.some(({ path }) => path.siblings.length !== height - levels)
      )
        throw new ClientError("CLIENT_INVALID_RPC_RESPONSE", {
          details: { method: method.name, path: "$" },
        });
      return response;
    } catch (cause) {
      throw ringIndexerError(cause, method.name);
    }
  }
}

function ringIndexerError(cause: unknown, method: string): Error {
  if (cause instanceof ApiError && cause.code === "API_JSON_RPC") {
    const rpcCode = cause.details?.["rpcCode"];
    const code = typeof rpcCode === "number" ? RING_PROJECTION_CODES.get(rpcCode) : undefined;
    if (code !== undefined) return new RingError(code, { details: { method }, cause });
  }
  return wrapIndexer(cause, method);
}
