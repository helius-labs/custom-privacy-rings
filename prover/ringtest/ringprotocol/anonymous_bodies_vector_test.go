package ringprotocol

import (
	"encoding/hex"
	"math/big"
	"testing"

	"github.com/helius-labs/zolana-custom-rings/prover/ringtest"
)

const anonymousBodiesDescription = "One sender bundle of an honest 4 slot frame, change in slot 0, payments in slots 1 and 3, an exit in slot 2. " +
	"Bundle plaintext output_seed || recipient_u per published slot under label CRING/snd and the CRDS slot secret of sender_viewing_key and index m, recipient_u zero on exit and dummy. " +
	"bundle_digest = ciphertext_digest(bundle). body_tag = Poseidon(CRRT, body_slot_secret), bundle_tag = Poseidon(CRST, bundle_slot_secret), dummy_tag = Poseidon(CRRT, dummy_tag_preimage). " +
	"bundle and bundle_digest produced by prover/ringtest/ringprotocol/anonymous_bodies_vector_test.go with WRITE_AUDIT_VECTORS=1, slot secrets and tags stay pinned in the file. Records are pinned in audit/audit-stream.json."

func TestAnonymousBodiesVector(t *testing.T) {
	var vector map[string]any
	ringtest.ReadVector(t, "audit/anonymous_bodies.json", &vector)
	text := func(name string) string { return vector[name].(string) }
	field := func(name string) *big.Int { return ringtest.VectorField(t, text(name)) }
	var keys [][32]byte
	for _, key := range vector["recipient_keys"].([]any) {
		var recipient [32]byte
		if text := key.(string); text != "" {
			decoded, err := hex.DecodeString(text)
			if err != nil {
				t.Fatal(err)
			}
			recipient = [32]byte(decoded)
		}
		keys = append(keys, recipient)
	}
	bundle := SenderBundle{
		OutputSeed:    field("output_seed"),
		RecipientKeys: keys,
	}.Seal(t, field("bundle_slot_secret"))
	vector["description"] = anonymousBodiesDescription
	for _, retired := range []string{"body", "body_blinding", "body_amount", "body_mint", "change_mint", "change_amount"} {
		delete(vector, retired)
	}
	vector["bundle"] = hex.EncodeToString(bundle)
	vector["bundle_digest"] = ringtest.FieldHex(EnvelopeDigest(t, bundle))
	ringtest.PinVector(t, "audit/anonymous_bodies.json", vector)
}
