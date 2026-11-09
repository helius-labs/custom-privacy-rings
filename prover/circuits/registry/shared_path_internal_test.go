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
	short := SharedQuadProof{Canopy: proof.Canopy, Paths: []SharedQuadPath{proof.Paths[0], {Siblings: proof.Paths[1].Siblings[1:]}}}
	if short.Validate(2) == nil {
		t.Fatal("a path that stops below the canopy accepted")
	}
}
