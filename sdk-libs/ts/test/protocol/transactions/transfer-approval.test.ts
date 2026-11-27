import { bytesToHex } from "@noble/hashes/utils.js";
import { AccountRole, getBase58Decoder, type Address } from "@solana/kit";
import { describe, expect, it } from "vitest";

import { ringCoSignerAddress, ringPolicyAddress } from "../../../src/protocol/pda/index.js";
import { RingInstructionTag } from "../../../src/protocol/transaction/instruction-tag.js";
import {
  MAX_TRANSFER_ACCOUNTS,
  TransferApproval,
  approveTransferInstruction,
} from "../../../src/protocol/transaction/transfer-approval.js";
import { filled, ringError } from "../../helpers/ring-pipeline.js";

const address = (seed: number): Address => getBase58Decoder().decode(filled(seed)) as Address;
const call = {
  ringProgramId: address(1),
  data: Uint8Array.of(RingInstructionTag.transfer, 7, 8),
  accounts: [address(2), address(3)],
};

describe("TransferApproval", () => {
  // Rust `TransferApproval::digest` and `encode` over the same input.
  it("digests and encodes the vector Rust produces", () => {
    expect(bytesToHex(TransferApproval.digest(call))).toBe(
      "2ed0f53961ae51c9a8751a719a070cc01715ad2b4d73ad2c2c59f2be419a9356",
    );
    expect(bytesToHex(TransferApproval.encode({ position: 3, digest: filled(9) }))).toBe(
      `4103${"09".repeat(32)}`,
    );
  });

  it("binds the program, the data and the account order", () => {
    const base = TransferApproval.digest(call);
    const other = (changed: Partial<typeof call>) =>
      TransferApproval.digest({ ...call, ...changed });
    expect(other({ ringProgramId: address(9) })).not.toEqual(base);
    expect(other({ data: Uint8Array.of(RingInstructionTag.transfer, 7, 9) })).not.toEqual(base);
    expect(other({ accounts: [address(3), address(2)] })).not.toEqual(base);
    // 32 data bytes moved into the account list.
    const data = new Uint8Array(32).fill(RingInstructionTag.transfer);
    expect(
      other({ data: Uint8Array.of(...data, ...filled(2)), accounts: [address(3)] }),
    ).not.toEqual(other({ data }));
  });

  it("refuses a list past the transfer ceiling", () => {
    const accounts = Array.from({ length: MAX_TRANSFER_ACCOUNTS + 1 }, () => address(2));
    expect(() => TransferApproval.digest({ ...call, accounts: accounts.slice(1) })).not.toThrow();
    expect(() => TransferApproval.digest({ ...call, accounts })).toThrow(
      ringError("RING_TRANSACT_DATA_INVALID", { reason: "TooManyAccounts" }),
    );
  });

  it("parses approval data only at its exact length and tag", () => {
    const approval = { position: 3, digest: filled(9) };
    const data = TransferApproval.encode(approval);
    expect(TransferApproval.parse(data)).toEqual(approval);
    expect(TransferApproval.parse(data.subarray(0, 33))).toBeUndefined();
    expect(TransferApproval.parse(Uint8Array.of(...data, 0))).toBeUndefined();
    expect(
      TransferApproval.parse(Uint8Array.of(RingInstructionTag.transfer, ...data.subarray(1))),
    ).toBeUndefined();
    expect(() => TransferApproval.encode({ ...approval, position: 256 })).toThrow(
      ringError("RING_TRANSACT_DATA_INVALID"),
    );
  });

  it("builds the instruction over the ring, the record and the signing co-signer", async () => {
    const [ringProgramId, coSigner] = [address(1), address(5)];
    const approval = { position: 3, digest: filled(9) };
    expect(await approveTransferInstruction({ ringProgramId, coSigner, approval })).toEqual({
      programAddress: ringProgramId,
      accounts: [
        { address: await ringPolicyAddress(ringProgramId), role: AccountRole.READONLY },
        { address: await ringCoSignerAddress(ringProgramId, coSigner), role: AccountRole.READONLY },
        { address: coSigner, role: AccountRole.READONLY_SIGNER },
      ],
      data: TransferApproval.encode(approval),
    });
  });
});
