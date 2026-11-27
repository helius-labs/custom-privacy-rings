package ringregistry

import (
	"math/big"
	"testing"

	"github.com/helius-labs/zolana-custom-rings/prover/circuits/registry"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest"
)

type quadTree struct {
	nodes []map[uint64]*big.Int
	empty []*big.Int
}

func newQuadTree(t testing.TB, height int) quadTree {
	t.Helper()
	tree := quadTree{nodes: make([]map[uint64]*big.Int, height+1), empty: make([]*big.Int, height+1)}
	tree.empty[0] = big.NewInt(0)
	for level := 1; level <= height; level++ {
		e := tree.empty[level-1]
		tree.empty[level] = ringtest.MustPoseidonHash(t, e, e, e, e)
	}
	for level := range tree.nodes {
		tree.nodes[level] = map[uint64]*big.Int{}
	}
	return tree
}

func (q quadTree) height() int {
	return len(q.nodes) - 1
}

func (q quadTree) root() *big.Int {
	return new(big.Int).Set(q.node(q.height(), 0))
}

func (q quadTree) node(level int, index uint64) *big.Int {
	if node, ok := q.nodes[level][index]; ok {
		return node
	}
	return q.empty[level]
}

func (q quadTree) setLeaf(t testing.TB, index uint64, leaf *big.Int) {
	t.Helper()
	q.nodes[0][index] = leaf
	for level := 0; level < q.height(); level++ {
		parent := index >> (2 * (level + 1))
		first := parent * registry.Arity
		q.nodes[level+1][parent] = ringtest.MustPoseidonHash(t, q.node(level, first), q.node(level, first+1), q.node(level, first+2), q.node(level, first+3))
	}
}

// siblings lists each level's other three children in child order, leaf level first.
func (q quadTree) siblings(index uint64) [][registry.Arity - 1]*big.Int {
	rows := make([][registry.Arity - 1]*big.Int, q.height())
	for level := range rows {
		node := index >> (2 * level)
		k := 0
		for child := node &^ (registry.Arity - 1); k < registry.Arity-1; child++ {
			if child != node {
				rows[level][k] = new(big.Int).Set(q.node(level, child))
				k++
			}
		}
	}
	return rows
}

type sortedRecord interface {
	member() *big.Int
	Hash(t testing.TB) *big.Int
}

// sortedTree keeps the sentinel at index 0 and appends every insert.
type sortedTree[R sortedRecord] struct {
	records []R
	tree    quadTree
}

func (s *sortedTree[R]) Root() *big.Int {
	return s.tree.root()
}

func (s *sortedTree[R]) NextIndex() uint64 {
	return uint64(len(s.records))
}

// find returns member's index, or the index of the record with the largest member below it.
func (s *sortedTree[R]) find(member *big.Int) (uint64, bool) {
	low := uint64(0)
	for i, record := range s.records {
		switch record.member().Cmp(member) {
		case 0:
			return uint64(i), true
		case -1:
			if record.member().Cmp(s.records[low].member()) > 0 {
				low = uint64(i)
			}
		}
	}
	return low, false
}

func (s *sortedTree[R]) set(t testing.TB, index uint64, record R) {
	t.Helper()
	if index == uint64(len(s.records)) {
		s.records = append(s.records, record)
	} else {
		s.records[index] = record
	}
	s.tree.setLeaf(t, index, record.Hash(t))
}

func (s *sortedTree[R]) sharedProof(newProof func(openings int) (registry.SharedQuadProof, error), leaves []uint64) (registry.SharedQuadProof, error) {
	proof, err := newProof(len(leaves))
	if err != nil {
		return registry.SharedQuadProof{}, err
	}
	depth := len(proof.Paths[0].Siblings)
	for i := range proof.Canopy {
		proof.Canopy[i] = new(big.Int).Set(s.tree.node(depth, uint64(i)))
	}
	for i, leaf := range leaves {
		proof.Paths[i].Index = leaf
		wireSiblings(proof.Paths[i].Siblings, s.tree.siblings(leaf)[:depth])
	}
	return proof, nil
}
