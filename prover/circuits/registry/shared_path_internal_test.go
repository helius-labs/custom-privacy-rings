package registry

import "testing"

func TestSharedQuadProofRefusesAMissingOrShortPath(t *testing.T) {
	proof, err := NewSharedQuadProof(2)
	if err != nil {
		t.Fatal(err)
	}
	if err := proof.Validate(2); err != nil {
		t.Fatal(err)
	}
	if proof.Validate(3) == nil {
		t.Fatal("a record without a path accepted")
	}
	short := SharedQuadProof{TreeHeight: proof.TreeHeight, Canopy: proof.Canopy, Paths: []SharedQuadPath{proof.Paths[0], {Siblings: proof.Paths[1].Siblings[1:]}}}
	if short.Validate(2) == nil {
		t.Fatal("a path that stops below the canopy accepted")
	}
}

func TestSharedQuadProofRefusesAHeightThatNamesNoTree(t *testing.T) {
	tracking, err := NewTrackingProof(2)
	if err != nil {
		t.Fatal(err)
	}
	unnamed := SharedQuadProof{Canopy: tracking.Canopy, Paths: tracking.Paths}
	if unnamed.Validate(2) == nil {
		t.Fatal("a pool with no height accepted")
	}
	relabeled := tracking
	relabeled.TreeHeight = Height
	if relabeled.Validate(2) == nil {
		t.Fatal("tracking paths accepted at the policy height")
	}
	if _, err := newSharedProof(2, 0); err == nil {
		t.Fatal("a zero height built a pool")
	}
}
