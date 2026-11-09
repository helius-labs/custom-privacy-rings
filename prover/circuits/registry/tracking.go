// Defines how enrollments sit in a ring's tracking tree, apart from its policy
// registry. SPP commits a note to any owner and nullifier key pair. A ring holds an
// owner to the key it enrolled, the auditor holds the secret behind that key, and the
// same leaf fixes the owner's list tag. Every circuit that reads an owner's enrolled
// key opens one of these leaves under the tracking root. The tree stands apart because
// a storage update names a revision, on one tree enrollment traffic would fail the
// restrictive policy writes it raced.
//
// Read after registry/registry.go, registry/register_key.go. Read before
// registry/apply_enrollments.go, policy/tracking.go, merge.go.

package registry

import (
	"github.com/consensys/gnark/frontend"
	"github.com/reilabs/gnark-lean-extractor/v3/abstractor"

	"github.com/helius-labs/zolana-custom-rings/prover/circuits/ringgadget"
	"github.com/helius-labs/zolana-custom-rings/prover/circuits/shared"
	"github.com/helius-labs/zolana/prover/server/circuits/gadget"
)

// Properties:
// 1. A leaf carries CRTK and one owner's nullifier key, ciphertext hash, list tag and viewing hash, its domain keeps it apart from a policy leaf. Only Next and ViewingHash move after insert, registry/apply_enrollments.go holds both.
// 2. Resolve reads a present owner's own leaf or proves the owner absent by the one link it falls inside, the sort order registry/apply_enrollments.go keeps leaves no third case.
// 3. An absent owner or an unset cond reads an all zero row.
// 4. The leaf folds to root while cond is set, and one index below Capacity names it.
// 5. No circuit decides on ViewingHash. A sender must check hash_bytes(viewing_u) against it off chain, an X25519 u can exceed the BN254 modulus.

// The empty leaf is 0, and the sentinel (0, q-1, 0, 0, 0, 0) at index 0 brackets every member in [1, q-2].
type TrackingRecord struct {
	Member, Next frontend.Variable
	NullifierPk  frontend.Variable
	CtHash       frontend.Variable
	ListTag      frontend.Variable
	ViewingHash  frontend.Variable
}

func (r TrackingRecord) Hash(api frontend.API) frontend.Variable {
	return gadget.PoseidonHash(api, []frontend.Variable{shared.TrackingLeafDomain, r.Member, r.Next, r.NullifierPk, r.CtHash, r.ListTag, r.ViewingHash})
}

// TrackingOpening opens one leaf, the owner's own when Present, else the low record bracketing the owner.
type TrackingOpening struct {
	Present frontend.Variable
	Record  TrackingRecord
	Path    QuadPath
}

// TrackingRow is zero for an absent owner or an unset cond.
type TrackingRow struct {
	Present, NullifierPk, CtHash, ListTag frontend.Variable
}

// Resolve binds the opening to root while the boolean cond is set.
func (o TrackingOpening) Resolve(api frontend.API, cond, root, member frontend.Variable) TrackingRow {
	// 1. Present selects exactly one branch below and keeps the row whole or zero.
	// The caller holds cond boolean, a fraction would scale the row and blend both branches.
	api.AssertIsBoolean(o.Present)
	present := api.Mul(cond, o.Present)
	// 2. A present owner's leaf names it, an absent owner lies strictly inside the opened leaf's link.
	abstractor.CallVoid(api, gadget.AssertEqualWhen{Cond: present, A: o.Record.Member, B: member})
	ringgadget.AssertOrderedWhen(api, api.Sub(cond, present), o.Record.Member, member, o.Record.Next)
	// 3. The leaf folds to root, a leaf of another ring's tree fails.
	abstractor.CallVoid(api, gadget.AssertEqualWhen{Cond: cond, A: o.Path.Root(api, o.Record.Hash(api)), B: root})
	return TrackingRow{
		Present:     present,
		NullifierPk: api.Mul(present, o.Record.NullifierPk),
		CtHash:      api.Mul(present, o.Record.CtHash),
		ListTag:     api.Mul(present, o.Record.ListTag),
	}
}
