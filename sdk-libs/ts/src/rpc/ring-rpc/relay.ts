import type { Address, Bytes32 } from "@heliuslabs/spr/interface";
import { copyBytes } from "@heliuslabs/spr/internal";
import type { RelayLeg } from "../../protocol/ring-rpc/relay-proof-inputs.js";
import { type RingReadTarget, targetLines, checkedReadTarget } from "./signed-request.js";
import { encoder, requestAddress, base58Decoder, invalidRequest } from "./wire.js";

const RELAY_DEPOSIT_DOMAIN = "zolana/ring-rpc-relay-deposit";
const RELAY_MERGE_DOMAIN = "zolana/ring-rpc-relay-merge";

/** Rust `RelayRequest`, `digest` is `depositRequestDigest` or `mergeRequestDigest` of the proof inputs. */
export type RelayRequest =
  | Readonly<{ kind: "deposit"; digest: Bytes32; legs: readonly RelayLeg[] }>
  | Readonly<{ kind: "merge"; digest: Bytes32 }>;

/** Mirrors Rust `RelayRequestAttestation::message`, the depositor or merge owner signs these bytes raw. */
export function relayRequestAttestation(
  input: Readonly<{ target: RingReadTarget; signer: Address; request: RelayRequest }>,
): Uint8Array {
  const { request } = input;
  const legs = request.kind === "deposit" ? request.legs : [];
  return encoder.encode(
    [
      request.kind === "deposit" ? RELAY_DEPOSIT_DOMAIN : RELAY_MERGE_DOMAIN,
      ...targetLines(checkedReadTarget(input.target)),
      `signer: ${requestAddress(input.signer, "signer")}`,
      ...legs.map(
        (leg) =>
          `${leg.isDeposit ? "deposit" : "withdrawal"}: ${requestAddress(leg.mint, "mint")} ${checkedU64(leg.amount, "amount")}`,
      ),
      `request: ${base58Decoder.decode(copyBytes(request.digest, 32, "digest"))}`,
    ].join("\n"),
  );
}

function checkedU64(value: bigint, field: string): bigint {
  if (typeof value !== "bigint" || value < 0n || value > 0xffff_ffff_ffff_ffffn)
    throw invalidRequest(field);
  return value;
}
