package ringregistry

import (
	"fmt"

	"github.com/helius-labs/zolana-custom-rings/prover/circuits/registry"
)

func (r *Registry) SharedProof(indices []uint64) (registry.SharedQuadProof, error) {
	for _, index := range indices {
		if index >= registry.Capacity {
			return registry.SharedQuadProof{}, fmt.Errorf("registry leaf index outside capacity")
		}
	}
	return r.sharedProof(registry.NewSharedQuadProof, indices)
}
