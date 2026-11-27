package ringpolicy

import (
	"math/big"
	"testing"

	"github.com/consensys/gnark/frontend"

	"github.com/helius-labs/zolana-custom-rings/prover/circuits/policy"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest/ringregistry"
)

// RingSalt opens the tag a ring salted table commits.
var RingSalt = big.NewInt(0x5a17)

func ListSaltIn(t testing.TB, owner, ringID *big.Int) *big.Int {
	t.Helper()
	return ringtest.MustPoseidonHash(t, new(big.Int).Add(owner, big.NewInt(4)), ringID)
}

func ListSaltOf(t testing.TB, owner *big.Int) *big.Int {
	t.Helper()
	return ListSaltIn(t, owner, big.NewInt(ringID))
}

// Enroll applies owner's leaf, the fields no owner predicate reads derive from it.
func Enroll(t testing.TB, tree *ringregistry.Tracking, owner, nullifierPk *big.Int) {
	t.Helper()
	derived := func(offset int64) *big.Int { return new(big.Int).Add(owner, big.NewInt(offset)) }
	tree.Apply(t, ringregistry.TrackingEntry{Member: owner, NullifierPk: nullifierPk, CtHash: derived(1), ListTag: ringregistry.ListTag(t, ListSaltOf(t, owner))})
}

// FillTracking opens each live member once and fills the pool cyclically.
func FillTracking(t testing.TB, target *policy.Tracking, tree *ringregistry.Tracking, rows, outputs []frontend.Variable, p *policy.Policy, payments []frontend.Variable, voucher frontend.Variable) {
	t.Helper()
	pool := trackingCollector(tree)
	entry := func(member frontend.Variable) frontend.Variable {
		return pool.add(member, ringtest.AsBigInt(member).Sign() != 0)
	}
	for j, member := range rows {
		target.RowEntries[j] = entry(member)
	}
	for o, member := range outputs {
		target.OutputEntries[o] = entry(member)
	}
	if p != nil {
		for j := range rows {
			p.SenderEntries[j].Tracking = target.RowEntries[j]
		}
		for o := range payments {
			p.OwnerEntries[o].Tracking = 0
			if ringtest.AsBigInt(p.Salted).Sign() != 0 {
				p.OwnerEntries[o].Tracking = entry(payments[o])
			}
		}
		p.VoucherEntry.Tracking = 0
		if ringtest.AsBigInt(p.Salted).Sign() != 0 {
			p.VoucherEntry.Tracking = entry(voucher)
		}
	}
	// The sentinel fills a pool nothing live opened.
	if len(pool.subjects) == 0 {
		pool.add(big.NewInt(0), true)
	}
	target.TrackingRoot = tree.Root()
	target.TrackingProof = pool.fill(t, target.TrackingRecords, target.TrackingSubjects)
}
