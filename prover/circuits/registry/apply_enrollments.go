// Proves that a batch of queued enrollments was applied to the tracking tree in
// queue order. Anyone may apply, the program folds the chain from the queue's entry
// hashes, so the proof only moves the tree. An enrollment grants nothing until its
// leaf sits under a root, and transfers, deposits and merges read an owner's key or
// list tag from that root. A tree out of order would let an enrolled owner prove
// itself unenrolled and skip its private lists.
//
// Read after registry/tracking.go, registry/register_key.go, registry/apply_records.go.
// Read before policy/tracking.go, prover/ring/ring.go.

package registry

import (
	"errors"

	"github.com/consensys/gnark/frontend"
	"github.com/reilabs/gnark-lean-extractor/v3/abstractor"

	"github.com/helius-labs/zolana-custom-rings/prover/circuits/ringgadget"
	"github.com/helius-labs/zolana-custom-rings/prover/circuits/shared"
	"github.com/helius-labs/zolana/prover/server/circuits/gadget"
)

// Properties:
// 1. NewRoot, NewNextIndex and ChainEnd follow from applying, in queue order, exactly the Count entries whose chain runs from ChainStart to ChainEnd.
// 2. A proven entry of an absent owner inserts after its low record at exactly the next index, below capacity. An unproven entry of an absent owner changes nothing.
// 3. An entry of a present owner moves only that owner's ViewingHash. Its nullifier key, ciphertext hash and list tag stay as first inserted, and a full tree still applies it.
// 4. Present is forced. A present owner falls inside no link, and an absent owner opens no leaf naming it, so an owner holds one leaf.
// 5. The statement binds both roots, both next indexes, both chain ends and the count.
//
// The program holds what the circuit takes as given. tracking_chain_start in program-libs/interface/src/accounts/tracking.rs names one registry at one revision in ChainStart, and Enroll sets Proven only after RegisterKey verifies.

// TrackingEntry mirrors TrackingEntry::hash in program-libs/interface/src/accounts/tracking.rs.
type TrackingEntry struct {
	Member, NullifierPk, CtHash, ListTag, ViewingHash frontend.Variable
	// Proven is one when Enroll verified RegisterKey, the one bit that lets an entry insert.
	Proven frontend.Variable
}

func (e TrackingEntry) Hash(api frontend.API) frontend.Variable {
	return gadget.PoseidonHash(api, []frontend.Variable{shared.TrackingEntryDomain, e.Member, e.NullifierPk, e.CtHash, e.ListTag, e.ViewingHash, e.Proven})
}

type EnrollmentWrite struct {
	Entry   TrackingEntry
	Present frontend.Variable
	// The owner's own leaf when Present, else the leaf bracketing it.
	Low     TrackingRecord
	LowSlot QuadPath
	// The empty leaf an insert takes, any other write binds only its index range.
	Slot QuadPath
}

// ApplyEnrollmentsCircuit only moves the tree, the program computes each entry and anyone may apply.
type ApplyEnrollmentsCircuit struct {
	NumWrites int `gnark:"-"`

	PublicInputHash frontend.Variable `gnark:",public"`

	OldRoot      frontend.Variable
	NewRoot      frontend.Variable
	OldNextIndex frontend.Variable
	NewNextIndex frontend.Variable
	ChainStart   frontend.Variable
	ChainEnd     frontend.Variable
	Count        frontend.Variable
	Writes       []EnrollmentWrite
}

func NewApplyEnrollmentsCircuit(writes int) (*ApplyEnrollmentsCircuit, error) {
	if err := shared.ValidateWidth("tracking", writes, ApplyWidths); err != nil {
		return nil, err
	}
	return &ApplyEnrollmentsCircuit{NumWrites: writes, Writes: make([]EnrollmentWrite, writes)}, nil
}

func (c *ApplyEnrollmentsCircuit) Define(api frontend.API) error {
	if err := errors.Join(shared.ValidateWidth("tracking", c.NumWrites, ApplyWidths), shared.ValidateLength("tracking", "write", len(c.Writes), c.NumWrites)); err != nil {
		return err
	}
	// 1. Count selects a prefix of one write up to the capacity, the writes past it act on nothing.
	count := ringgadget.SelectCount(api, c.Count, 1, c.NumWrites)
	// 2. Every active write moves the running root, next index and chain in the named order.
	state := registryState{root: c.OldRoot, next: c.OldNextIndex, chain: c.ChainStart}
	for i, w := range c.Writes {
		state = w.apply(api, count.Active[i], state)
	}
	// 3. The tree after the batch and the entries it consumed are the statement's.
	api.AssertIsEqual(state.root, c.NewRoot)
	api.AssertIsEqual(state.next, c.NewNextIndex)
	api.AssertIsEqual(state.chain, c.ChainEnd)
	api.AssertIsEqual(c.PublicInputHash, gadget.HashChain4(api, c.statement()))
	return nil
}

func (w EnrollmentWrite) apply(api frontend.API, active frontend.Variable, s registryState) registryState {
	// 1. Present and Proven are bits, a fraction would blend an insert with an update.
	api.AssertIsBoolean(w.Present)
	api.AssertIsBoolean(w.Entry.Proven)
	update := api.Mul(active, w.Present)
	absent := api.Sub(active, update)
	insert := api.Mul(absent, w.Entry.Proven)
	// 2. A present owner's own leaf names it, an absent owner lies strictly inside its low record's link.
	// The sort argument of registry/apply_records.go applies, the links descend from the sentinel's (0, q-1) by one split per insert. A present owner falls inside no link and an absent owner opens no leaf naming it, so Present cannot lie.
	abstractor.CallVoid(api, gadget.AssertEqualWhen{Cond: update, A: w.Low.Member, B: w.Entry.Member})
	ringgadget.AssertOrderedWhen(api, absent, w.Low.Member, w.Entry.Member, w.Low.Next)
	// 3. Low opens under the running root and its replacement relinks on an insert or takes the new viewing hash on an update.
	// The nullifier key, ciphertext hash and list tag of Low carry over, an update never moves the sealed fields.
	replaced := w.Low
	replaced.Next = api.Select(insert, w.Entry.Member, w.Low.Next)
	replaced.ViewingHash = api.Select(update, w.Entry.ViewingHash, w.Low.ViewingHash)
	lowRoot, linked := w.LowSlot.Update(api, w.Low.Hash(api), replaced.Hash(api))
	abstractor.CallVoid(api, gadget.AssertEqualWhen{Cond: active, A: lowRoot, B: s.root})
	// 4. An insert takes the empty leaf 0 at the next index under the relinked root, linking on to the low record's old next.
	// At next = Capacity the index fails QuadPath.digits, a full tree takes no new owner. Any other write binds Slot only to an index in range, so a full tree still applies updates.
	abstractor.CallVoid(api, gadget.AssertEqualWhen{Cond: insert, A: w.Slot.Index, B: s.next})
	after := TrackingRecord{
		Member: w.Entry.Member, Next: w.Low.Next, NullifierPk: w.Entry.NullifierPk, CtHash: w.Entry.CtHash, ListTag: w.Entry.ListTag, ViewingHash: w.Entry.ViewingHash,
	}.Hash(api)
	opened, written := w.Slot.Update(api, 0, after)
	abstractor.CallVoid(api, gadget.AssertEqualWhen{Cond: insert, A: opened, B: linked})
	// 5. The chain absorbs every active entry, a skipped one included, so the program consumes exactly the queue prefix it hashed.
	chain := gadget.PoseidonHash(api, []frontend.Variable{s.chain, w.Entry.Hash(api)})
	return registryState{
		root:  api.Select(insert, written, api.Select(active, linked, s.root)),
		next:  api.Add(s.next, insert),
		chain: api.Select(active, chain, s.chain),
	}
}

// statement mirrors ApplyEnrollmentsStatement::elements in program-libs/interface/src/proof/statement.rs.
func (c *ApplyEnrollmentsCircuit) statement() []frontend.Variable {
	return []frontend.Variable{
		shared.ApplyEnrollmentsDomain, c.OldRoot, c.NewRoot, c.OldNextIndex, c.NewNextIndex, c.ChainStart, c.ChainEnd, c.Count,
	}
}
