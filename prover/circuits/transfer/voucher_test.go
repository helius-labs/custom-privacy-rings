package transfer_test

import (
	"math/big"
	"testing"

	"github.com/helius-labs/zolana-custom-rings/prover/circuits/transfer"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest/ringaudit"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest/ringtransfer"
	"github.com/helius-labs/zolana/prover/server/prover-test/spp/spptest"
)

func vouchedWorld() ringtransfer.Confidential {
	w := ringtransfer.Signed(confidential2)
	w.Voucher, w.Unlisted = ringaudit.VoucherKey, true
	return w
}

// One listed signer admits a sender and owners no allow entry names, and nobody passes without it.
func TestVoucherAdmitsUnlistedOwners(t *testing.T) {
	requireSolved(t, vouchedWorld().Build(t).Circuit)
	t.Run("no voucher", func(t *testing.T) {
		w := vouchedWorld()
		w.Voucher = nil
		refused(t, w.Build(t).Circuit, "policy.assertPaymentRules")
	})
	t.Run("a throwaway key in the listed voucher's place", func(t *testing.T) {
		b := vouchedWorld().Build(t)
		unvouched := b.Audit
		unvouched.Voucher = nil
		b.Circuit.Voucher = unvouched.VoucherWires(t, ringtest.AsBigInt(b.Circuit.PrivateTxHash), ringtest.AsBigInt(b.Circuit.WithdrawalHash))
		refused(t, b.Circuit, "policy.subjectPool.read")
	})
}

// A signature that fails the curve equation leaves the gadget's hinted limbs out of range, the refusal surfaces where the range table closes.
// Every nonnative gadget closes there, each case below changes the voucher wires alone of a build the test first solves.
const rangeTable = "nonnative.(*rangeTable).build"

// A vouch is a signature over one ring and one transfer's hashes by the key the proof names.
func TestVouchBindsTheTransfer(t *testing.T) {
	requireSolved(t, vouchedWorld().Build(t).Circuit)
	hashes := func(b ringtransfer.Built) (privateTxHash, withdrawalHash *big.Int) {
		return ringtest.AsBigInt(b.Circuit.PrivateTxHash), ringtest.AsBigInt(b.Circuit.WithdrawalHash)
	}
	t.Run("a signature over another transfer", func(t *testing.T) {
		b := vouchedWorld().Build(t)
		privateTxHash, withdrawalHash := hashes(b)
		b.Circuit.Voucher = b.Audit.VoucherWires(t, new(big.Int).Add(privateTxHash, big.NewInt(1)), withdrawalHash)
		refused(t, b.Circuit, rangeTable)
	})
	t.Run("a signature over other withdrawal legs", func(t *testing.T) {
		b := vouchedWorld().Build(t)
		privateTxHash, withdrawalHash := hashes(b)
		b.Circuit.Voucher = b.Audit.VoucherWires(t, privateTxHash, new(big.Int).Add(withdrawalHash, big.NewInt(1)))
		refused(t, b.Circuit, rangeTable)
	})
	t.Run("a signature made for another ring", func(t *testing.T) {
		b := vouchedWorld().Build(t)
		refused(t, onAnotherRing(t, b), rangeTable)
	})
	// No note of the frame carries the ring id, the digest alone names the ring.
	t.Run("a signature made for another ring over default-ring notes alone", func(t *testing.T) {
		b := defaultRingWorld(t).Build(t)
		for _, input := range b.Circuit.Inputs {
			if ringtest.AsBigInt(input.RingProgramID).Sign() != 0 {
				t.Fatal("the frame spends a ring note")
			}
		}
		for _, output := range b.Circuit.Outputs {
			if ringtest.AsBigInt(output.Output.RingProgramID).Sign() != 0 {
				t.Fatal("the frame pays a ring note")
			}
		}
		requireSolved(t, b.Circuit)
		refused(t, onAnotherRing(t, defaultRingWorld(t).Build(t)), rangeTable)
	})
	t.Run("another key's signature", func(t *testing.T) {
		b := vouchedWorld().Build(t)
		unvouched := b.Audit
		unvouched.Voucher = nil
		privateTxHash, withdrawalHash := hashes(b)
		b.Circuit.Voucher.Signature = unvouched.VoucherWires(t, privateTxHash, withdrawalHash).Signature
		refused(t, b.Circuit, rangeTable)
	})
	t.Run("an unvouched transfer under a forged signature", func(t *testing.T) {
		b := ringtransfer.Signed(confidential2).Build(t)
		b.Circuit.Voucher.Signature[0] = ringtest.AsBigInt(b.Circuit.Voucher.Signature[0]).Uint64() ^ 1
		refused(t, b.Circuit, "ed25519.Verify")
	})
}

// onAnotherRing replaces the vouch by the same key's signature over the same hashes under another ring id.
func onAnotherRing(t *testing.T, b ringtransfer.Built) *transfer.TransferCircuit {
	t.Helper()
	other := b.Audit
	other.RingProgramID = new(big.Int).Add(other.RingProgramID, big.NewInt(1))
	b.Circuit.Voucher = other.VoucherWires(t, ringtest.AsBigInt(b.Circuit.PrivateTxHash), ringtest.AsBigInt(b.Circuit.WithdrawalHash))
	return b.Circuit
}

// defaultRingWorld spends one default-ring note into exits on an entry key, a vouched frame with no ring note.
func defaultRingWorld(t *testing.T) ringtransfer.Confidential {
	t.Helper()
	w := ringtransfer.Signed(confEntry2)
	w.Voucher, w.Unlisted = ringaudit.VoucherKey, true
	w.Exits = []int{0, 1, 2, 3}
	w.Inputs = []ringaudit.Input{{Utxo: ringaudit.SpentUtxo(t, ringtransfer.Sender(t), spptest.MustNullifierPk(t, big.NewInt(8)), big.NewInt(0), 70, 53), OwnerKey: ringtransfer.SenderKey(), Mint: ringaudit.FixtureMint}}
	return w
}
