package shared_test

import (
	"math/big"
	"testing"

	"github.com/consensys/gnark/frontend"

	"github.com/helius-labs/zolana-custom-rings/prover/circuits/shared"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest"
)

type packDisclosureCircuit struct {
	Bytes  [40]frontend.Variable
	Fields [2]frontend.Variable `gnark:",public"`
}

func (c *packDisclosureCircuit) Define(api frontend.API) error {
	for i, field := range shared.PackDisclosure(api, c.Bytes[:]) {
		api.AssertIsEqual(field, c.Fields[i])
	}
	return nil
}

// A 40-byte slot fills one chunk and leaves its last 9 bytes first in the second chunk, the zero padding after them.
func TestPackDisclosureLeftAlignsTheTail(t *testing.T) {
	var raw [40]byte
	witness := &packDisclosureCircuit{}
	for i := range raw {
		raw[i] = byte(0xc0 + i)
		witness.Bytes[i] = raw[i]
	}
	witness.Fields[0] = new(big.Int).SetBytes(raw[:31])
	witness.Fields[1] = new(big.Int).Lsh(new(big.Int).SetBytes(raw[31:]), 8*22)
	ringtest.RequireSolved(t, &packDisclosureCircuit{}, witness)
	witness.Fields[1] = new(big.Int).SetBytes(raw[31:])
	ringtest.RequireUnsolved(t, &packDisclosureCircuit{}, witness)
}
