import { ClientError, type RequestContext } from "@heliuslabs/spr";
import { ApiError, equalBytes, wrapIndexer, type SprApi } from "@heliuslabs/spr/internal";

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
} from "../protocol/indexer/tracking-codec.js";
import type { ServedTrackingPool } from "../protocol/membership/tracking.js";
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
    const method = "getTrackingRecordPool";
    try {
      return await this.#api.call(getTrackingRecordPoolMethod, request, context);
    } catch (cause) {
      throw ringIndexerError(cause, method);
    }
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
    const method = "getStorageRecordPool";
    try {
      const response = await this.#api.call(getStorageRecordPoolMethod, request, context);
      if (
        !equalBytes(response.root.root, request.expectedRoot) ||
        response.records.length < 1 ||
        response.records.length > request.openings ||
        response.canopy.length !== QUAD_ARITY ** canopyLevels(request.openings, QUAD_HEIGHT) ||
        response.subjects.length !== request.members.length
      )
        throw new ClientError("CLIENT_INVALID_RPC_RESPONSE", { details: { method, path: "$" } });
      return response;
    } catch (cause) {
      throw ringIndexerError(cause, method);
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
