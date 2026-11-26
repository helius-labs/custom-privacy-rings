import type { Address } from "@heliuslabs/spr/interface";
import type { X25519PublicKey } from "@heliuslabs/spr/keypair";
import { record, invalid } from "./wire.js";

/** Mirrors Rust `RingState`. */
export type RingState = "served" | "foreignAuditor" | "uninitialized";

/** Mirrors Rust `RingActivation`, SPP's gate on every operational ring instruction. */
export interface RingActivation {
  readonly activated: boolean;
  readonly paused: boolean;
}

/** Mirrors Rust `RingStatusResponse`, unsigned and only what the chain publishes. */
export interface RingStatus {
  readonly ringProgramId: Address;
  readonly state: RingState;
  /** The key the ring account names, absent before `Initialize`. */
  readonly configAuditorPublicKey?: X25519PublicKey;
  readonly servicePublicKey: Address;
  /** Absent until SPP holds a config for the ring. */
  readonly activation?: RingActivation;
}

export function decodeActivation(value: unknown): RingActivation {
  const wire = record(value, "result.activation");
  const flag = (name: string): boolean => {
    const field = wire[name];
    if (typeof field !== "boolean") throw invalid(`result.activation.${name}`);
    return field;
  };
  return Object.freeze({
    activated: flag("activated"),
    paused: flag("paused"),
  });
}
