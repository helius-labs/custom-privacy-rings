package transfer_test

import (
	"math/big"
	"testing"

	"github.com/helius-labs/zolana-custom-rings/prover/circuits/shared"
	"github.com/helius-labs/zolana-custom-rings/prover/circuits/transfer"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest/ringaudit"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest/ringtransfer"
	"github.com/helius-labs/zolana/prover/server/prover-test/spp/spptest"
)

// Eight signing rows, two outputs and thirty two unenrolled withdrawal owners under a salted list fill the wide key's tracking pool.
func TestTheWidestInlineSpendOpensEveryTrackingRow(t *testing.T) {
	w := ringtransfer.Signed(wideOwners2)
	w.SaltedBlock, w.Exits, w.Omitted = []*big.Int{}, []int{}, wideOwners2.Outputs-2
	for k := range 7 {
		w.CoOwners = append(w.CoOwners, big.NewInt(0x1000+int64(k)))
	}
	asset := ringtransfer.Asset(t)
	for leg := range 32 {
		w.Withdrawals = append(w.Withdrawals, ringtransfer.Withdrawal{Owner: big.NewInt(0x2000 + int64(leg)), Asset: asset, Amount: 1})
	}
	w.Public = []ringtransfer.PublicSlot{{Asset: asset, Net: -32, Gross: 32}}
	built := w.Build(t)
	keyed, err := transfer.Keyed(wideOwners2)
	if err != nil {
		t.Fatal(err)
	}
	live := 0
	for _, subject := range built.Circuit.TrackingSubjects {
		if ringtest.AsBigInt(subject.Member).Sign() != 0 {
			live++
		}
	}
	if live != keyed.Tracking || len(built.Circuit.TrackingSubjects) != keyed.Tracking {
		t.Fatalf("%d live rows in a pool of %d, want %d in %d", live, len(built.Circuit.TrackingSubjects), keyed.Tracking, keyed.Tracking)
	}
	requireSolved(t, built.Circuit)
	// A dropped subject leaves its read unanswered.
	built.Circuit.TrackingSubjects[len(built.Circuit.TrackingSubjects)-1].Member = 0
	refused(t, built.Circuit)
}

var payout16 = transfer.Shape{Inputs: 4, Outputs: 16, Withdrawals: 4, Owners: 4}

// Four owner rows, sixteen distinct recipients and four leg owners under a salted list fill the 16 output key's tracking pool.
func TestTheWidestPayoutOpensEveryTrackingRow(t *testing.T) {
	for name, recipientKey := range map[string]func(i int) *big.Int{
		"zero key recipients, an absence query each": func(int) *big.Int { return shared.ZeroNullifierPk },
		"enrolled recipients, a salted read each":    func(i int) *big.Int { return spptest.MustNullifierPk(t, big.NewInt(0x300+int64(i))) },
	} {
		t.Run(name, func(t *testing.T) {
			w := ringtransfer.Signed(payout16)
			w.SaltedBlock, w.Exits = []*big.Int{}, []int{}
			ring := ringID(t)
			w.Inputs = []ringaudit.Input{senderNote(t, 51)}
			for k := range 3 {
				coOwner := big.NewInt(0x1000 + int64(k))
				w.CoOwners = append(w.CoOwners, coOwner)
				w.Inputs = append(w.Inputs, ringaudit.Input{Utxo: ringaudit.SpentUtxo(t, coOwner, ringtransfer.CoOwnerKey(t, coOwner), ring, 1, 53+int64(k))})
			}
			w.Outputs = map[int]func(*ringaudit.Slot){}
			for i := range payout16.Outputs {
				owner, key := ringtest.Repeat(byte(0x30+i)), recipientKey(i)
				w.Outputs[i] = func(slot *ringaudit.Slot) {
					slot.OwnerKey, slot.Utxo.OwnerPkHash, slot.Utxo.NullifierPk = owner, ringtest.MustSolanaPkField(t, owner), key
				}
			}
			asset := ringtransfer.Asset(t)
			for leg := range payout16.Withdrawals {
				w.Withdrawals = append(w.Withdrawals, ringtransfer.Withdrawal{Owner: big.NewInt(0x2000 + int64(leg)), Asset: asset, Amount: 1})
			}
			w.Public = []ringtransfer.PublicSlot{{Asset: asset, Net: -int64(payout16.Withdrawals), Gross: uint64(payout16.Withdrawals)}}
			built := w.Build(t)
			keyed, err := transfer.Keyed(payout16)
			if err != nil {
				t.Fatal(err)
			}
			if keyed.Tracking != 24 || keyed.Records != 64 {
				t.Fatalf("pools [%d, %d], want [24, 64]", keyed.Tracking, keyed.Records)
			}
			live := 0
			for _, subject := range built.Circuit.TrackingSubjects {
				if ringtest.AsBigInt(subject.Member).Sign() != 0 {
					live++
				}
			}
			if live != keyed.Tracking {
				t.Fatalf("%d live rows, want %d", live, keyed.Tracking)
			}
			requireSolved(t, built.Circuit)
		})
	}
}
