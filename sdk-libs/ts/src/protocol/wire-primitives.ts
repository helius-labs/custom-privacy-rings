import { U64_MAX, indexerScalars, wireDecoder } from "@heliuslabs/spr/internal";
import type { Bytes32 } from "@heliuslabs/spr/interface";

export type WireFault = "type" | "integer";
export type WireRefusal = (path: string, fault: WireFault, value: unknown) => never;

export interface WirePrimitives {
  text(value: unknown, path: string): string;
  /** A JSON number, or the decimal text the transport quotes past `Number.MAX_SAFE_INTEGER`. */
  u64(value: unknown, path: string): bigint;
  /** A JSON number from zero through `maximum`. */
  count(value: unknown, path: string, maximum: number): number;
  /** The decimal text a wallet document stores. */
  decimalU64(value: unknown, path: string): bigint;
  /** 32 bytes in their one base58 spelling. */
  hash(value: unknown, path: string): Bytes32;
}

const DECIMAL = /^(?:0|[1-9][0-9]*)$/;
const SAFE_MAX = BigInt(Number.MAX_SAFE_INTEGER);

export function wirePrimitives(refuse: WireRefusal): WirePrimitives {
  const { base58 } = wireDecoder((path) => refuse(path, "type", undefined));
  const decimal = (value: unknown): bigint | undefined => {
    if (typeof value !== "string" || !DECIMAL.test(value)) return undefined;
    const parsed = BigInt(value);
    return parsed > U64_MAX ? undefined : parsed;
  };
  const count = (value: unknown, path: string, maximum: number): number =>
    typeof value === "number" && Number.isSafeInteger(value) && value >= 0 && value <= maximum
      ? value
      : refuse(path, "integer", value);
  return Object.freeze({
    text: (value: unknown, path: string): string =>
      typeof value === "string" ? value : refuse(path, "type", value),
    u64(value: unknown, path: string): bigint {
      if (typeof value === "number") return BigInt(count(value, path, Number.MAX_SAFE_INTEGER));
      const quoted = decimal(value);
      return quoted !== undefined && quoted > SAFE_MAX ? quoted : refuse(path, "integer", value);
    },
    count,
    decimalU64: (value: unknown, path: string): bigint =>
      decimal(value) ?? refuse(path, "integer", value),
    hash(value: unknown, path: string): Bytes32 {
      const bytes = base58(value, path);
      return bytes.length === 32 ? (bytes as Bytes32) : refuse(path, "type", value);
    },
  });
}

/** The primitives of a codec that refuses with the indexer schema codes. */
export const schemaWire = wirePrimitives((path, fault, value) =>
  fault === "integer"
    ? indexerScalars.schemaFailure("INDEXER_SCHEMA_INVALID_INTEGER", path, "a u64", value)
    : indexerScalars.schemaFailure("INDEXER_SCHEMA_INVALID_TYPE", path, "a string", value),
);

/** JSON text `JSON.stringify` writes in place. */
export type RawJson = Readonly<{ rawJSON: string }>;

/** The engine floor holds `JSON.rawJSON` and a reviver's `context.source`, the TS lib types neither. */
const engineJson = JSON as unknown as Readonly<{
  rawJSON(text: string): RawJson;
  parse(
    text: string,
    reviver: (key: string, value: unknown, context: Readonly<{ source?: string }>) => unknown,
  ): unknown;
}>;

/** A request `u64` in its exact digits, the number Rust serde reads. */
export function toU64(value: bigint, path: string): RawJson {
  if (typeof value !== "bigint" || value < 0n || value > U64_MAX)
    return indexerScalars.schemaFailure(
      "INDEXER_SCHEMA_INVALID_INTEGER",
      path,
      `an integer from 0 through ${U64_MAX}`,
      value,
    );
  return engineJson.rawJSON(value.toString());
}

const INTEGER_LITERAL = /^(?:0|-?[1-9][0-9]*)$/;

/** `JSON.parse` that hands `integer` each integer literal's digits and refuses any other number. */
export function parseExactJson(text: string, integer: (digits: string) => unknown): unknown {
  return engineJson.parse(text, (_key, value, context) => {
    if (typeof value !== "number") return value;
    const digits = context.source;
    if (digits === undefined || !INTEGER_LITERAL.test(digits))
      throw new SyntaxError("a number is not an integer literal");
    return integer(digits);
  });
}
