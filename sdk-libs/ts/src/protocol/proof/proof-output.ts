import {
  createProofOutput,
  type ProofOutputInit,
  type ProofOutputUtxo,
} from "@heliuslabs/spr/transaction";
import { defined } from "../defined.js";

export function withOutputFields(
  output: ProofOutputUtxo,
  patch: Partial<ProofOutputInit>,
): ProofOutputUtxo {
  return createProofOutput({
    ...defined({ ownerAddress: output.ownerAddress }),
    asset: output.asset,
    ...defined({ mint: output.mint }),
    amount: output.amount,
    blinding: output.blinding,
    data: output.data,
    ...defined({
      dataHash: output.dataHash,
      ringDataHash: output.ringDataHash,
      ringProgramId: output.ringProgramId,
      ownerTag: output.ownerTag,
      cacheSlot: output.cacheSlot,
    }),
    ...(output.compact === true ? { compact: true } : {}),
    ...patch,
  });
}
