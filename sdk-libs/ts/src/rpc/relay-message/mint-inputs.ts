import { equalBytes, isCanonicalField, ZERO_32 } from "@heliuslabs/spr/internal";
import { ShieldedPublicKey } from "@heliuslabs/spr/keypair";
import { privateTxBlinding, privateTxHash, Utxo } from "@heliuslabs/spr/transaction";
import { ringBlindingSeed } from "../../crypto/derivation/blinding.js";
import { RecordKind, type OutputRecord } from "../../protocol/codec/audit-stream.js";
import { RingTransactData } from "../../protocol/transaction/transact.js";
import { type ExpectedDelegate, mismatch, SourceSpends, withMismatch } from "./expected.js";

export function mintInputSpends({
  expected,
  frame,
  records,
  realInputs,
}: Readonly<{
  expected: ExpectedDelegate;
  frame: RingTransactData;
  records: readonly OutputRecord[];
  realInputs: number;
}>): SourceSpends {
  const { mint, inputOpenings } = expected;
  if (
    mint === undefined ||
    inputOpenings === undefined ||
    realInputs === 0 ||
    realInputs !== inputOpenings.length ||
    records.length !== frame.outputs.length
  ) {
    throw mismatch("SpendsUnrequestedNote");
  }
  return withMismatch("SpendsUnrequestedNote", () => {
    const spends = new SourceSpends(new Map());
    const inputHashes = inputOpenings.map((opening) => {
      const source = expected.sources[opening.sourceIndex];
      const ringDataHash = opening.ringDataHash ?? ZERO_32;
      if (
        source === undefined ||
        !isCanonicalField(source.nullifierPk) ||
        !isCanonicalField(opening.blinding) ||
        !isCanonicalField(ringDataHash)
      )
        throw mismatch("SpendsUnrequestedNote");
      const note = new Utxo({
        owner: ShieldedPublicKey.fromEd25519(source.ownerKey),
        asset: mint,
        amount: opening.amount,
        blinding: opening.blinding,
        ringProgramId: expected.ring,
      });
      spends.credit(source, mint, opening.amount);
      return note.proofInput(source.nullifierPk, opening.treeId, ZERO_32, ringDataHash).hash();
    });
    const outputHashes = frame.outputs.flatMap((output, index) =>
      records[index]?.kind === RecordKind.dummy ? [] : [output.hash],
    );
    const seed = ringBlindingSeed(expected.txViewingKey, frame.salt);
    try {
      const blinding = privateTxBlinding(RingTransactData.firstNullifier(frame), seed);
      try {
        if (
          !equalBytes(privateTxHash({ inputHashes, outputHashes, blinding }), frame.privateTxHash)
        )
          throw mismatch("SpendsUnrequestedNote");
      } finally {
        blinding.fill(0);
      }
    } finally {
      seed.fill(0);
    }
    return spends;
  });
}
