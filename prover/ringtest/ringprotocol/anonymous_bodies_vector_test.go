package ringprotocol

import (
	"encoding/binary"
	"encoding/hex"
	"math/big"
	"slices"
	"testing"

	"github.com/helius-labs/zolana-custom-rings/prover/circuits/audit"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest"
	"github.com/helius-labs/zolana/prover/server/prover-test/spp/spptest"
)

const anonymousBodiesDescription = "One recipient record of an honest 4 slot frame, change in slot 0, payments in slots 1 and 3, an exit in slot 2. " +
	"body is the first 105 ciphertext bytes of the slot 1 record u8(1) || u64be(body_amount) || body_owner_key || body_mint || be32(body_blinding) || be32(body_nullifier_pk) under label CRING/rcp and body_slot_secret, the CRDS slot secret of recipient_keys[body_index] and body_index, with no payload. " +
	"recipient_keys[0] is the sender's own key for its change, the sender reads every recipient_u from the auditor stream. " +
	"body, recipient_keys and the record fields are produced by prover/ringtest/ringprotocol/anonymous_bodies_vector_test.go with WRITE_AUDIT_VECTORS=1, the keys and the slot secret stay pinned in the file."

// Slot 3's recipient key and slot 1's TXOB blinding.
const (
	secondPayee  = "bc7d2c70bbbaa5da20d0ae93ed02530960f0e34cd388c137db752490b6c8215a"
	bodyBlinding = "1fbf7826244b3fc62c8a17a93aae8199401ae503b62fcb983d19ea98c777e331"
)

func TestAnonymousBodiesVector(t *testing.T) {
	var pinned map[string]any
	ringtest.ReadVector(t, "audit/anonymous_bodies.json", &pinned)
	// A field the host neither pins nor produces leaves the file on a rewrite.
	vector := map[string]any{}
	for _, name := range []string{"tx_viewing_sk", "recipient_viewing_sk", "sender_viewing_key", "salt", "first_nullifier", "output_count", "body_index", "body_slot_secret"} {
		vector[name] = pinned[name]
	}
	text := func(name string) string { return vector[name].(string) }
	field := func(name string) *big.Int { return ringtest.VectorField(t, text(name)) }
	key := func(value string) [32]byte {
		decoded, err := hex.DecodeString(value)
		if err != nil {
			t.Fatal(err)
		}
		return [32]byte(decoded)
	}
	txViewingSk, recipientSk := key(text("tx_viewing_sk")), key(text("recipient_viewing_sk"))
	salt := [16]byte(ringtest.VectorBytes(t, text("salt"), 16))
	firstNullifier := field("first_nullifier")
	outputs := int(vector["output_count"].(float64))
	bodyIndex := int(vector["body_index"].(float64))
	// Key k names slot k, the change slot carries the sender's own key.
	keys := [][32]byte{key(text("sender_viewing_key")), XPublicKey(t, recipientSk), {}, key(secondPayee)}
	if len(keys) != outputs {
		t.Fatalf("%d keys for %d slots", len(keys), outputs)
	}
	// The pinned slot secret is the one its reader derives from its own key.
	if DeliveredSlotSecret(t, txViewingSk, keys[bodyIndex], salt, bodyIndex, firstNullifier).Cmp(field("body_slot_secret")) != 0 {
		t.Fatal("body_slot_secret is not slot body_index's delivered secret")
	}
	ownerKey, mint := ringtest.Repeat(0xb3), ringtest.Repeat(0x0a)
	nullifierPk := spptest.MustNullifierPk(t, big.NewInt(21))
	blinding := key(bodyBlinding)
	amount := uint64(31)
	nullifierBytes := spptest.MustFieldBytes(t, nullifierPk)
	record := slices.Concat([]byte{1}, binary.BigEndian.AppendUint64(nil, amount), ownerKey[:], mint[:], blinding[:], nullifierBytes[:])
	recordKey, nonce := AuditStreamKeys(t, field("body_slot_secret"), "CRING/rcp")
	body := CTREncrypt(t, recordKey, nonce, record)[:audit.RingRecordLen]
	recipientKeys := make([]string, len(keys))
	for slot, recipient := range keys {
		if recipient != [32]byte{} {
			recipientKeys[slot] = hex.EncodeToString(recipient[:])
		}
	}
	vector["description"] = anonymousBodiesDescription
	vector["recipient_keys"] = recipientKeys
	vector["body_owner_key"] = hex.EncodeToString(ownerKey[:])
	vector["body_blinding"] = bodyBlinding
	vector["body_nullifier_pk"] = ringtest.FieldHex(nullifierPk)
	vector["body_amount"] = amount
	vector["body_mint"] = hex.EncodeToString(mint[:])
	vector["body"] = hex.EncodeToString(body)
	ringtest.PinVector(t, "audit/anonymous_bodies.json", vector)
}
