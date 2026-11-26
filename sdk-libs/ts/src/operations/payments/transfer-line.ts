/**
 * A transfer serves one line of builder values, a step checks then writes then advances, a transfer
 * a ring step wrote into is never taken by a new line.
 */
import type { ConfidentialTransfer } from "@heliuslabs/spr/transaction";

import { RingError } from "../../protocol/error.js";

type Steps = { taken: number; bound: boolean };

const position = Symbol("transfer line position");

/** Builder values at one step of the line that holds their `transfer`. */
export type TransferLine<Values> = Values &
  Readonly<{ transfer: ConfidentialTransfer; [position]: Readonly<{ steps: Steps; at: number }> }>;

const lines = new WeakMap<ConfidentialTransfer, Steps>();

/** A second line supersedes the first until a ring step wrote into `transfer`, then it is refused. */
export function openLine<Values extends Readonly<{ transfer: ConfidentialTransfer }>>(
  values: Values,
  write: (transfer: ConfidentialTransfer) => void,
): TransferLine<Values> {
  const steps = lines.get(values.transfer) ?? { taken: 0, bound: false };
  if (steps.bound) throw new RingError("RING_TRANSFER_SUPERSEDED");
  write(values.transfer);
  lines.set(values.transfer, steps);
  steps.taken += 1;
  return { ...values, [position]: { steps, at: steps.taken } };
}

/** Rust consumes a builder on each step, here a value another step has passed is refused. */
export function current<Values>(line: TransferLine<Values>): TransferLine<Values> {
  const { steps, at } = line[position];
  if (at !== steps.taken) throw new RingError("RING_TRANSFER_SUPERSEDED");
  return line;
}

/** `write` alone changes the shared transfer, every earlier value is superseded once it returns. */
export function step<Values, Written>(
  line: TransferLine<Values>,
  check: (values: TransferLine<Values>) => TransferLine<Values>,
  write: (checked: TransferLine<Values>) => Written,
): Readonly<{ line: TransferLine<Values>; written: Written }> {
  const checked = check(current(line));
  const written = write(checked);
  const { steps } = checked[position];
  steps.taken += 1;
  steps.bound = true;
  return { line: { ...checked, [position]: { steps, at: steps.taken } }, written };
}
