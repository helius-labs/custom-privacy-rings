// Proves that a ring's SPP merge output opens to the ring's auditor.
//
// SPP's merge proof ties every input to one owner, one asset and the sum, and it seals nothing to an
// auditor. The ring proof adds the transfer's output record of the merged note, followed by the merge
// seed, sealed to the owner's viewing key from a fresh ephemeral key. An envelope seals that ephemeral
// key and the viewing key to the ring's auditor. It opens no input, so one key serves every merge width.
// Without it the auditor holds no proven opening of a merged note.
//
// SPP derives the output blinding as TMOB(seed, n0) and binds no output ciphertext on a ring merge,
// zolana/prover/server/circuits/spp_merge/ring.go. The seed is the owner's nullifier secret. The record
// seals that seed, the auditor already holds it through enrollment. The ring proof binds SPP's output
// ciphertext to the same seed, amount and mint, so the owner's wallet rebuilds the output from the
// SPP merge alone.
//
// The merge statement proves the following for one SPP merge output named by its hash and first nullifier.
//  1. The envelope seals eph_sk and the recipient key to the ring's auditor key, and the record is the
//     output's record and seed under the slot secret of eph_sk and that recipient key. The auditor rebuilds
//     the published hash from the record and the tree id and ring data hash SPP publishes.
//  2. The owner's nullifier key is its enrolled row's under the tracking root. A zero key is never
//     enrolled, so no zero-key merge proves. The payer may be the hidden owner, a wallet that pays
//     from its own account names itself to every chain reader.
//  3. SPP's output ciphertext is the amount and mint under the key of the seed and first nullifier.
//
// SPP's proof ties every input to that owner, asset and sum. No list, guard, cap or co-signer rule is proven.
//
// SPP's merge publishes no auditor message and no external data hash covers the record. The auditor
// reads both ciphertexts from the ring instruction that called SPP in the confirmed transaction, and
// the statement alone binds those bytes. The record's label CRING/mro keeps a merge record apart from a
// transfer record under one key pair.

package circuits

import (
	"slices"

	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/std/rangecheck"

	"github.com/helius-labs/zolana-custom-rings/prover/circuits/audit"
	"github.com/helius-labs/zolana-custom-rings/prover/circuits/registry"
	"github.com/helius-labs/zolana-custom-rings/prover/circuits/shared"
	"github.com/helius-labs/zolana/prover/server/circuits/gadget"
	mergeshared "github.com/helius-labs/zolana/prover/server/circuits/spp_merge/shared"
	transaction "github.com/helius-labs/zolana/prover/server/circuits/spp_transaction/shared"
)

const MergeRecordLen = audit.MergeRecordLen

// MergeCircuit binds a ring's SPP merge output to a record its owner and its auditor open, SPP's proof ties every input to that owner, asset and sum.
type MergeCircuit struct {
	PublicInputHash frontend.Variable `gnark:",public"`

	OutputUtxoHash frontend.Variable
	FirstNullifier frontend.Variable
	RingProgramID  frontend.Variable
	AuditorPk      [32]frontend.Variable
	Payer          frontend.Variable
	TrackingRoot   frontend.Variable
	// A 16-byte salt is canonical without a range check.
	OutputCiphertextSalt frontend.Variable

	EphSk [32]frontend.Variable
	// RecipientKey is the merged owner's viewing u, the envelope seals it for the auditor.
	RecipientKey [32]frontend.Variable
	// The record's encoding binds OwnerPkHash to OwnerKey and Asset to MintChunks.
	OwnerKey    [32]frontend.Variable
	OwnerPkHash frontend.Variable
	MintChunks  [2]frontend.Variable
	Asset       frontend.Variable
	Amount      frontend.Variable
	NullifierPk frontend.Variable
	// The owner's nullifier secret, the output hash holds it to SPP's blinding.
	Seed               frontend.Variable
	OutputTreeID       frontend.Variable
	OutputRingDataHash frontend.Variable
	Tracking           registry.TrackingOpening
}

// No list, guard, cap or co-signer rule is proven, a merge keeps one owner's value in one asset.
func (c *MergeCircuit) Define(api frontend.API) error {
	// 1. The owner's key is its row's, register_key.go enrolls no zero key.
	row := c.Tracking.Resolve(api, 1, c.TrackingRoot, c.OwnerPkHash)
	api.AssertIsEqual(row.Present, 1)
	api.AssertIsEqual(row.NullifierPk, c.NullifierPk)

	// 2. The output is the SPP merge output, the program reads OutputUtxoHash from the SPP body it forwards and the auditor rebuilds it from the record.
	blinding := mergeshared.MergeOutputBlinding(api, c.Seed, c.FirstNullifier)
	api.AssertIsEqual(c.OutputUtxoHash, transaction.UtxoHashCircuit(api, transaction.UtxoCircuitFields{
		Domain: mergeshared.UtxoDomain, Owner: gadget.PoseidonHash(api, []frontend.Variable{c.OwnerPkHash, c.NullifierPk}),
		Asset: c.Asset, Amount: c.Amount, Blinding: blinding,
		DataHash: 0, RingDataHash: c.OutputRingDataHash, RingProgramID: c.RingProgramID,
	}, c.OutputTreeID))

	// 3. The envelope gives the auditor eph_sk and the recipient key, the record then opens as a transfer record does.
	envelope := shared.Envelope{Plaintext: slices.Concat(c.EphSk[:], c.RecipientKey[:]), EphSk: c.EphSk, AuditorPk: c.AuditorPk, Info: shared.MergeEncInfo}.SealStream(api)
	rc := rangecheck.New(api)
	plaintext := audit.MergeRecord{RecordOpening: audit.RecordOpening{
		Live: 1, Kind: audit.KindRing, Amount: c.Amount, OwnerPkHash: c.OwnerPkHash, NullifierPk: c.NullifierPk, Asset: c.Asset, Blinding: blinding,
		OwnerKey: c.OwnerKey, MintChunks: c.MintChunks,
	}, Seed: c.Seed}.Encode(api, rc)
	// A zero salt and index leave the first nullifier to part two merges of one key pair.
	record := shared.SlotSeal{
		Plaintext: plaintext, EphSk: c.EphSk, Eph: envelope.Eph,
		RecipientPk: c.RecipientKey, Salt: 0, Index: 0, FirstNullifier: c.FirstNullifier, Info: shared.MergeOutputInfo,
	}.Seal(api)

	// 4. SPP publishes the ciphertext unproven, a relayer's altered bytes fail here.
	ciphertext := mergeshared.MergeOutputCiphertext(api, c.Seed, c.FirstNullifier, c.OutputCiphertextSalt, c.Amount, c.MintChunks)

	// 5. The statement names the SPP merge, the ring and the three ciphertexts, mirrored by MergePublicInput::elements in program-libs/interface/src/instructions/merge.rs.
	api.AssertIsEqual(c.PublicInputHash, gadget.HashChain(api, slices.Concat(
		[]frontend.Variable{shared.MergeStatementDomain, c.OutputUtxoHash, c.FirstNullifier, c.RingProgramID},
		envelope.StatementFields(api),
		[]frontend.Variable{gadget.HashBoundedBytes(api, record), c.Payer, c.TrackingRoot, c.OutputCiphertextSalt},
		ciphertext,
	)))
	return nil
}
