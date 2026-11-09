package registry_test

import (
	"fmt"
	"math/big"
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/test"

	"github.com/helius-labs/zolana-custom-rings/prover/circuits/registry"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest/ringregistry"
)

type rootCircuit struct {
	Root, Leaf frontend.Variable
	Path       registry.QuadPath
}

func (c *rootCircuit) Define(api frontend.API) error {
	api.AssertIsEqual(c.Path.Root(api, c.Leaf), c.Root)
	return nil
}

// MEMBER_REGISTRY_EMPTY_ROOT in program-libs/interface/src/accounts/member_registry.rs, the sentinel leaf of eight inputs, and
// TRACKING_EMPTY_ROOT in program-libs/interface/src/accounts/tracking.rs, the CRTK sentinel leaf of seven.
func TestEmptyRootMatchesRust(t *testing.T) {
	for _, tree := range []struct {
		root *big.Int
		rust string
	}{
		{ringregistry.New(t).Root(), "295ec16f1ff587f0269eb21ddf079a2b44cb3e0c6852f35431ab02f7651ac967"},
		{ringregistry.NewTracking(t).Root(), "197ce7e4cc6e23ed6f4d5b32303dd91e032f9afdc7f8dd424ab2638d245e4559"},
	} {
		if got := fmt.Sprintf("%064x", tree.root); got != tree.rust {
			t.Fatalf("empty root %s, Rust %s", got, tree.rust)
		}
	}
}

func TestQuadPathFoldsTheHostTree(t *testing.T) {
	members := ringregistry.New(t)
	for m := int64(1); m <= 20; m++ {
		members.Apply(t, ringregistry.Entry{Member: big.NewInt(m * 10), Mask: big.NewInt(1), Value: big.NewInt(1)})
	}
	root := members.Root()
	for _, member := range []int64{10, 20, 30, 40, 50, 160, 170, 200} {
		opening := members.Opening(big.NewInt(member))
		assignment := &rootCircuit{Root: root, Leaf: opening.Record.Hash(t), Path: opening.Path.Wires()}
		if err := test.IsSolved(&rootCircuit{}, assignment, ecc.BN254.ScalarField()); err != nil {
			t.Fatalf("member %d at %d: %v", member, opening.Path.Index, err)
		}
	}
	// An empty leaf opens under the same root, its digits reach the top levels.
	for _, index := range []uint64{21, topDigit, 2 * topDigit, 3 * topDigit, registry.Capacity - 1} {
		assignment := &rootCircuit{Root: root, Leaf: 0, Path: members.Path(index).Wires()}
		if err := test.IsSolved(&rootCircuit{}, assignment, ecc.BN254.ScalarField()); err != nil {
			t.Fatalf("empty leaf %d: %v", index, err)
		}
	}
}

func TestQuadPathRejectsAnotherIndex(t *testing.T) {
	members := ringregistry.New(t)
	members.Apply(t, ringregistry.Entry{Member: big.NewInt(10), Mask: big.NewInt(1), Value: big.NewInt(1)})
	opening := members.Opening(big.NewInt(10))
	for _, tc := range []struct {
		name   string
		index  *big.Int
		frames []string
	}{
		{"a neighbour sharing the siblings", big.NewInt(2), []string{"[assertIsEqual]", "registry_test.(*rootCircuit).Define"}},
		{"the index plus the capacity", new(big.Int).Add(big.NewInt(1), big.NewInt(registry.Capacity)), []string{"[ToBinary]", "registry.QuadPath.digits"}},
	} {
		t.Run(tc.name, func(t *testing.T) {
			path := opening.Path.Wires()
			path.Index = tc.index
			err := test.IsSolved(&rootCircuit{}, &rootCircuit{Root: members.Root(), Leaf: opening.Record.Hash(t), Path: path}, ecc.BN254.ScalarField())
			ringtest.RequireRefused(t, err, tc.frames...)
		})
	}
}
