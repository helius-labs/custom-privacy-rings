package ringtransfer

import (
	"math/big"
	"testing"

	"github.com/helius-labs/zolana-custom-rings/prover/circuits/transfer"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest"
)

type Rail int

const (
	Signer Rail = iota
	InProof
	Delegate
)

// Confidential is one transfer of the confidential family.
type Confidential struct {
	World
	Shape transfer.Shape
	Rail  Rail
}

// Signed is a policy ring transfer on the signer rail of shape.
func Signed(shape transfer.Shape) Confidential {
	return Confidential{Shape: shape, Rail: Signer, World: World{Policy: Policy{Screened: true, Cap: 1_000_000, CosignAbove: 500_000, WithdrawalThreshold: 100_000}}}
}

func (w Confidential) Build(t testing.TB) Built {
	t.Helper()
	c, err := transfer.NewTransferCircuit(w.Shape)
	if err != nil {
		t.Fatal(err)
	}
	a := w.newAudit(t, w.Shape.Stream())
	sender := Sender(t)
	var run []*big.Int
	switch w.Rail {
	case Signer:
		run = append([]*big.Int{sender}, w.CoOwners...)
	case InProof:
		run = w.CoOwners
	}
	c.InProof, c.Hidden, c.Delegate, c.Sender, c.DelegateAsset = ringtest.Bit(w.Rail == InProof), 0, ringtest.Bit(w.Rail == Delegate), 0, w.delegateAsset()
	for i := range c.PartyIdentities {
		c.PartyIdentities[i] = 0
	}
	if w.Rail == InProof {
		c.Sender = sender
	}
	b := Built{Circuit: c, World: w.World, Assembled: w.fill(t, &c.Sections, a, w.Rail == Delegate, run)}
	b.Rehash(t)
	return b
}
