// Lets one allowlisted signer stand in for a missing allow entry of every sender and payment owner of a transfer.
//
// SPP checks an owner's signature and reads no list. A ring's allow rule reads each owner's own record, so a
// wallet or a program address its user never listed fails the rule though the user holds a listed key. The
// voucher is that listed key. It signs the transfer inside the proof, its key stays a private wire, and the
// auditor reads it from the stream, audit/audit.go.
//
// The signature covers shared.VouchDigest, the ring, the notes and the withdrawal legs of one transfer, so a
// vouch for one transfer admits no other and no transfer of another ring. The voucher answers for the whole transfer, no wire names a covered subject.
//
// Read after policy/subjects.go, shared/derivation.go. Read before policy/rules.go, audit/audit.go.
//
// Properties:
// 1. Key signs the vouch digest of the ring id and the transfer's private_tx_hash and withdrawal_hash on every proof, vouched or not.
// 2. Identity is the Solana owner identity of Key, the member the voucher's records read at.
// 3. Active is set only under Screened and only for a signer on Allow or PrivateAllow, so the key the stream seals is a listed key.
// 4. While Active, the voucher meets every sender rule over its own records.
// 5. While Active, a sender and a payment owner read the voucher's bit on Allow and PrivateAllow beside their own, every other list reads the subject's own record alone and an asset reads no voucher bit.
// 6. A clear Active reads the voucher off every list, every subject's view stays as its own records give it.

package policy

import (
	"github.com/consensys/gnark/frontend"
	"github.com/reilabs/gnark-lean-extractor/v3/abstractor"

	"github.com/helius-labs/zolana-custom-rings/prover/circuits/ringgadget"
	"github.com/helius-labs/zolana/prover/server/circuits/crypto/ed25519"
	"github.com/helius-labs/zolana/prover/server/circuits/gadget"
)

// The list indexes a vouch covers, list i+1 sits at index i, mirrored by ListSet::VOUCHED in program-libs/policy/src/list.rs.
var vouchedLists = [...]int{0, 7}

// VoucherWires are an Ed25519 key and its signature, an unvouched proof carries a throwaway pair.
type VoucherWires struct {
	PublicKey [32]frontend.Variable
	Signature [64]frontend.Variable
}

// Vouch is a verified voucher, Active is the layout bit the program rebuilds from the frame.
type Vouch struct {
	Active, Identity frontend.Variable
	// Key holds the verified bytes, the stream seals the same wires.
	Key [32]frontend.Variable
}

// Open verifies the signature over digest and names the signer.
func (w VoucherWires) Open(api frontend.API, rc frontend.Rangechecker, active, digest frontend.Variable) Vouch {
	// 1. FieldToBytesBEWith holds the one be32 below q, a second encoding would be a second message for one transfer.
	message := ringgadget.FieldToBytesBEWith(api, rc, digest)
	// 2. Verify's SHA-512 transcript bounds every key and message byte, the identity and the stream read bounded bytes.
	key := ed25519.Verify(api, ed25519.Inputs{PublicKey: w.PublicKey, Signature: w.Signature, Message: message[:]})
	return Vouch{Active: active, Identity: gadget.SolanaOwnerIdentityBounded(api, key), Key: key}
}

// assertVouches holds an active voucher to a list it covers, an unlisted signer lends no bit and the auditor would still read its key as the transfer's voucher.
func (v subjectView) assertVouches(api frontend.API, active frontend.Variable) {
	listed := frontend.Variable(0)
	for _, list := range vouchedLists {
		listed = api.Add(listed, v.onList[list])
	}
	abstractor.CallVoid(api, gadget.AssertZeroWhen{Cond: active, V: api.IsZero(listed)})
}

// cover merges the voucher's allow bits into a subject's view, a block or frozen bit stays the subject's own.
// A set bit adds admission only under a present mask, under an absent or alternative mask it removes admission, so a vouch loosens no deny rule.
func (v subjectView) cover(api frontend.API, voucher subjectView) subjectView {
	for _, list := range vouchedLists {
		v.onList[list] = api.Or(v.onList[list], voucher.onList[list])
	}
	return v
}
