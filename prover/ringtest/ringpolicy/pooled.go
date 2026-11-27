package ringpolicy

import (
	"math/big"
	"slices"
	"testing"

	"github.com/consensys/gnark/frontend"
	"github.com/helius-labs/zolana-custom-rings/prover/circuits/policy"
	circuitregistry "github.com/helius-labs/zolana-custom-rings/prover/circuits/registry"
	"github.com/helius-labs/zolana-custom-rings/prover/circuits/shared"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest/ringregistry"
	transaction "github.com/helius-labs/zolana/prover/server/circuits/spp_transaction/shared"
)

type poolCollector[R any] struct {
	open           func(member *big.Int) (leaf uint64, record R)
	prove          func(leaves []uint64) (circuitregistry.SharedQuadProof, error)
	records        []R
	leaves         []uint64
	recordIndices  map[uint64]int
	subjects       []policy.PoolSubject
	subjectIndices map[string]int
}

// OwnRecord is the own pool record entry resolves to.
func (b Witness) OwnRecord(entry policy.PoolEntry) *circuitregistry.Record {
	policyWitness := b.Policy()
	subject := policyWitness.OwnSubjects[ringtest.AsBigInt(entry.Own).Int64()]
	return &policyWitness.OwnRecords[ringtest.AsBigInt(subject.RecordIndex).Int64()]
}

// FillPools opens each live subject and record once and fills the pools cyclically to capacity.
func FillPools(t testing.TB, policyWitness *policy.Policy, outputs, inputs []shared.UtxoWires, senders []frontend.Variable, voucher frontend.Variable, ownTree, curatorTree *ringregistry.Registry, tracking *ringregistry.Tracking) {
	t.Helper()
	own, curator := registryCollector(ownTree), registryCollector(curatorTree)
	foreign, saltedOwn, saltedCurated := false, false, false
	saltedLists := ringtest.AsBigInt(policyWitness.Salted)
	for i, source := range policyWitness.Sources {
		curated := ringtest.AsBigInt(source.Registry).Sign() != 0
		foreign = foreign || curated
		if saltedLists.Bit(i) == 1 {
			saltedOwn, saltedCurated = saltedOwn || !curated, saltedCurated || curated
		}
	}
	ringSalted := ringtest.AsBigInt(policyWitness.RingListTag).Sign() != 0
	policyWitness.RingSalt = 0
	if ringSalted {
		policyWitness.RingSalt = RingSalt
	}
	entry := func(member frontend.Variable, active bool) policy.PoolEntry {
		return policy.PoolEntry{Own: own.add(member, active), Curator: [1]frontend.Variable{curator.add(member, active && foreign)}}
	}
	owner := func(member frontend.Variable, active bool) policy.OwnerEntry {
		identity := ringtest.AsBigInt(member)
		ownerEntry := policy.OwnerEntry{PoolEntry: entry(ringregistry.PublicKey(t, identity), active), Tracking: 0, ListSalt: 0, OwnSalted: 0, CuratorSalted: [1]frontend.Variable{0}, OwnRing: 0, CuratorRing: [1]frontend.Variable{0}}
		if !active || !(saltedOwn || saltedCurated) {
			return ownerEntry
		}
		if tracking.Opening(identity).Present {
			salt := ListSaltOf(t, identity)
			key := ringregistry.ListKey(t, identity, salt)
			ownerEntry.ListSalt = salt
			ownerEntry.OwnSalted = own.add(key, saltedOwn)
			ownerEntry.CuratorSalted[0] = curator.add(key, saltedCurated)
		}
		if ringSalted {
			key := ringregistry.RingListKey(t, identity, RingSalt)
			ownerEntry.OwnRing = own.add(key, saltedOwn)
			ownerEntry.CuratorRing[0] = curator.add(key, saltedCurated)
		}
		return ownerEntry
	}
	for j, sender := range senders {
		policyWitness.SenderEntries[j] = owner(sender, ringtest.AsBigInt(sender).Sign() != 0)
	}
	policyWitness.VoucherEntry = owner(voucher, ringtest.AsBigInt(voucher).Sign() != 0)
	for i, out := range outputs {
		active := ringtest.AsBigInt(out.Domain).Int64() == transaction.UtxoDomain
		policyWitness.OwnerEntries[i], policyWitness.AssetEntries[i] = owner(out.OwnerPkHash, active), entry(out.Asset, active)
	}
	for i, out := range policyWitness.Withdrawals {
		active := int64(i) < ringtest.AsBigInt(policyWitness.WithdrawalCount).Int64()
		index := len(outputs) + i
		policyWitness.OwnerEntries[index], policyWitness.AssetEntries[index] = owner(out.Owner, active), entry(out.Asset, active)
	}
	for i := range policyWitness.PublicAssetEntries {
		asset := policyWitness.PublicAssets[i]
		policyWitness.PublicAssetEntries[i] = own.add(asset, ringtest.AsBigInt(asset).Sign() != 0)
	}
	for i, input := range inputs {
		policyWitness.AssetEntries[len(policyWitness.OwnerEntries)+i] = entry(input.Asset, ringtest.AsBigInt(input.Domain).Int64() == transaction.UtxoDomain)
	}
	if len(curator.records) == 0 {
		curator.add(slices.Concat(senders, []frontend.Variable{0})[0], true)
	}
	policyWitness.OwnProof = own.fill(t, policyWitness.OwnRecords, policyWitness.OwnSubjects)
	policyWitness.CuratorProof = curator.fill(t, policyWitness.CuratorRecords, policyWitness.CuratorSubjects)
}

func registryCollector(tree *ringregistry.Registry) *poolCollector[circuitregistry.Record] {
	open := func(member *big.Int) (uint64, circuitregistry.Record) {
		opening := tree.Opening(member)
		return opening.Path.Index, opening.Record.Wires()
	}
	return newCollector(open, tree.SharedProof)
}

func trackingCollector(tree *ringregistry.Tracking) *poolCollector[circuitregistry.TrackingRecord] {
	open := func(member *big.Int) (uint64, circuitregistry.TrackingRecord) {
		opening := tree.Opening(member)
		return opening.Path.Index, opening.Record.Wires()
	}
	return newCollector(open, tree.SharedProof)
}

func newCollector[R any](open func(*big.Int) (uint64, R), prove func([]uint64) (circuitregistry.SharedQuadProof, error)) *poolCollector[R] {
	return &poolCollector[R]{open: open, prove: prove, recordIndices: map[uint64]int{}, subjectIndices: map[string]int{}}
}

func (c *poolCollector[R]) add(member frontend.Variable, active bool) int {
	if !active {
		return 0
	}
	identity := ringtest.AsBigInt(member)
	if index, ok := c.subjectIndices[identity.String()]; ok {
		return index
	}
	leaf, record := c.open(identity)
	recordIndex, ok := c.recordIndices[leaf]
	if !ok {
		recordIndex = len(c.records)
		c.recordIndices[leaf] = recordIndex
		c.records = append(c.records, record)
		c.leaves = append(c.leaves, leaf)
	}
	index := len(c.subjects)
	c.subjectIndices[identity.String()] = index
	c.subjects = append(c.subjects, policy.PoolSubject{Member: member, RecordIndex: recordIndex})
	return index
}

func (c *poolCollector[R]) fill(t testing.TB, target []R, subjects []policy.PoolSubject) circuitregistry.SharedQuadProof {
	t.Helper()
	if len(c.records) > len(target) {
		t.Fatalf("pool needs %d records with capacity %d", len(c.records), len(target))
	}
	if len(c.subjects) > len(subjects) {
		t.Fatalf("pool needs %d subjects with capacity %d", len(c.subjects), len(subjects))
	}
	for i := range subjects {
		subjects[i] = c.subjects[i%len(c.subjects)]
	}
	leaves := make([]uint64, len(target))
	for i := range target {
		target[i] = c.records[i%len(c.records)]
		leaves[i] = c.leaves[i%len(c.leaves)]
	}
	proof, err := c.prove(leaves)
	if err != nil {
		t.Fatal(err)
	}
	return proof
}
