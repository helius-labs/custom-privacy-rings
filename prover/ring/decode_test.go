package ring

import (
	"encoding/json"
	"fmt"
	"maps"
	"reflect"
	"strings"
	"testing"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark/frontend"

	"github.com/helius-labs/zolana-custom-rings/prover/circuits/policy"
	"github.com/helius-labs/zolana-custom-rings/prover/circuits/registry"
	"github.com/helius-labs/zolana-custom-rings/prover/circuits/transfer"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest/ringregistry"
	"github.com/helius-labs/zolana/prover/server/prover/common"
)

// A request names every value its circuit reads, an omitted one is never assigned zero.
func TestDecodeNamesEveryMissingValue(t *testing.T) {
	for _, key := range Keys {
		_, err := decode(key.Circuit, []byte(`{"circuitType":"`+string(key.Circuit)+`"}`))
		if err == nil || !strings.Contains(err.Error(), `missing field "publicInputHash"`) {
			t.Errorf("%s: an empty request decoded: %v", key.Circuit, err)
		}
	}
}

// Each case changes one value of an honest request.
func TestDecodeRejectsMalformedRequests(t *testing.T) {
	requests := map[common.CircuitType]any{transferKey: hostRequest(t, keyOf(t, transferKey)), batchKey: hostRequest(t, keyOf(t, batchKey))}
	output := func(m map[string]any) map[string]any { return m["outputs"].([]any)[0].(map[string]any) }
	write := func(m map[string]any) map[string]any { return m["writes"].([]any)[0].(map[string]any) }
	modulus := "0x" + ecc.BN254.ScalarField().Text(16)
	canonical := "not canonical lowercase 0x hex"
	for _, tc := range []struct {
		name    string
		circuit common.CircuitType
		tamper  func(map[string]any)
		want    string
	}{
		{"unknown field", transferKey, func(m map[string]any) { m["externalDataHash"] = m["firstNullifier"] }, `unknown field "externalDataHash"`},
		{"settled away field", transferKey, func(m map[string]any) { m["seedOut"] = m["firstNullifier"] }, `unknown field "seedOut"`},
		{"unknown nested field", transferKey, func(m map[string]any) { output(m)["owner"] = m["firstNullifier"] }, `unknown field "owner"`},
		{"field name in another case", transferKey, func(m map[string]any) { m["FirstNullifier"] = m["firstNullifier"]; delete(m, "firstNullifier") }, `unknown field "FirstNullifier"`},
		{"null layout", transferKey, func(m map[string]any) { m["layout"] = nil }, "layout: null value"},
		{"missing field", transferKey, func(m map[string]any) { delete(m, "firstNullifier") }, `missing field "firstNullifier"`},
		{"missing layout", transferKey, func(m map[string]any) { delete(m, "layout") }, `missing field "layout"`},
		{"missing flag", batchKey, func(m map[string]any) { delete(write(m), "exists") }, `writes: [0]: missing field "exists"`},
		{"explicit null approval", transferKey, func(m map[string]any) { m["approvalBlinder"] = nil }, "approvalBlinder: null value"},
		{"null active write", batchKey, func(m map[string]any) { m["writes"].([]any)[0] = nil }, "missing active write"},
		{"null field", transferKey, func(m map[string]any) { m["firstNullifier"] = nil }, "firstNullifier: null value"},
		{"null bytes", transferKey, func(m map[string]any) { m["auditorPk"] = nil }, "auditorPk: null value"},
		{"field at the modulus", transferKey, func(m map[string]any) { m["firstNullifier"] = modulus }, canonical},
		{"uppercase hex", transferKey, func(m map[string]any) { m["auditorPk"] = strings.ToUpper(m["auditorPk"].(string)) }, canonical},
		{"short field", transferKey, func(m map[string]any) { m["firstNullifier"] = "0x01" }, canonical},
		{"long salt", transferKey, func(m map[string]any) { m["salt"] = m["salt"].(string) + "00" }, "Salt: expected 16 values"},
		{"layout as a string", transferKey, func(m map[string]any) { m["layout"] = "2" }, "layout of type uint8"},
		{"layout above a byte", transferKey, func(m map[string]any) { m["layout"] = 256 }, "number 256"},
		{"an output past the capacity", transferKey, func(m map[string]any) { m["outputs"] = append(m["outputs"].([]any), output(m)) }, "Outputs: expected 2 values"},
		{"an owner key one byte short", transferKey, func(m map[string]any) { output(m)["ownerKey"] = output(m)["ownerKey"].(string)[:64] }, "OwnerKey: expected 32 values"},
		{"another key's request", transferKey, func(m map[string]any) { m["circuitType"] = "custom-ring-transfer-4-in24-w4-s4" }, "custom-ring-transfer-4-in24-w4-s4 request on the custom-ring-transfer-2-in24-w4-s4 key"},
		{"a write past the capacity", batchKey, func(m map[string]any) { m["writes"] = append(m["writes"].([]any), write(m)) }, "Writes: expected 16 values"},
		{"a path one sibling short", batchKey, func(m map[string]any) { write(m)["path"] = write(m)["path"].([]any)[1:] }, "Writes[0].Slot.Siblings[12]: expected 3 values"},
		{"a path one sibling long", batchKey, func(m map[string]any) { write(m)["lowPath"] = append(write(m)["lowPath"].([]any), m["oldRoot"]) }, "Writes[0].LowSlot.Siblings: expected 13 values"},
		{"lists as a string", batchKey, func(m map[string]any) { write(m)["oldLists"] = "1" }, "oldLists of type uint64"},
		{"exists as a number", batchKey, func(m map[string]any) { write(m)["exists"] = 1 }, "exists of type bool"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			var raw map[string]any
			if err := json.Unmarshal(mustMarshal(t, requests[tc.circuit]), &raw); err != nil {
				t.Fatal(err)
			}
			tc.tamper(raw)
			if _, err := decode(tc.circuit, mustMarshal(t, raw)); err == nil || !strings.Contains(err.Error(), tc.want) {
				t.Fatalf("want %q, got %v", tc.want, err)
			}
		})
	}
	honest := mustMarshal(t, requests[transferKey])
	if _, err := decode(transferKey, append(honest, []byte("{}")...)); err == nil {
		t.Fatal("trailing data decoded")
	}
	repeated := append([]byte(`{"firstNullifier":"0x`+strings.Repeat("00", 32)+`",`), honest[1:]...)
	if _, err := decode(transferKey, repeated); err == nil || !strings.Contains(err.Error(), `repeated field "firstNullifier"`) {
		t.Fatalf("a repeated field decoded: %v", err)
	}
	if _, err := decode("custom-ring-transfer-4-in24-w4-s4", honest); err == nil {
		t.Fatal("a request for the 2-output key decoded on the 4-output key")
	}
	oversized := append(append([]byte(nil), honest...), make([]byte, MaxBodyBytes)...)
	for i := len(honest); i < len(oversized); i++ {
		oversized[i] = ' '
	}
	if err := Unmarshal(oversized, new(TransferParams)); err == nil || !strings.Contains(err.Error(), "request body exceeds") {
		t.Fatalf("an oversized body decoded: %v", err)
	}
	padded := append([]byte(`{"firstNullifier":"`+strings.Repeat("0", MaxBodyBytes)+`"}`), []byte{}...)
	if _, err := decode(transferKey, padded); err == nil || !strings.Contains(err.Error(), "request body exceeds") {
		t.Fatalf("an oversized json request decoded: %v", err)
	}
	var raw map[string]any
	if err := json.Unmarshal(honest, &raw); err != nil {
		t.Fatal(err)
	}
	zero := "0x" + strings.Repeat("00", 32)
	huge := make([]any, maxJSONArrayLen+1)
	for i := range huge {
		huge[i] = zero
	}
	raw["signers"] = huge
	if _, err := decode(transferKey, mustMarshal(t, raw)); err == nil || !strings.Contains(err.Error(), "array exceeds") {
		t.Fatalf("an oversized array decoded: %v", err)
	}
}

// The Rust encoder's table for a PrivateFrozen source, spliced into a host transfer request, reaches the circuit wires.
func TestRustPolicyTableReachesTheAssignment(t *testing.T) {
	var request, table map[string]any
	if err := json.Unmarshal(mustMarshal(t, hostRequest(t, keyOf(t, transferKey))), &request); err != nil {
		t.Fatal(err)
	}
	ringtest.ReadVector(t, "proof/ring-request-policy-table.json", &table)
	maps.Copy(request, table)
	assignment, err := decode(transferKey, mustMarshal(t, request))
	if err != nil {
		t.Fatal(err)
	}
	wires := assignment.(*transfer.TransferCircuit).Table
	sent := uint16(table["salted"].(float64))
	if sent&ringregistry.PrivateFrozenBit == 0 || wires.Salted != frontend.Variable(sent) {
		t.Fatalf("salted %v, the Rust table sends %d", wires.Salted, sent)
	}
	if wires.Sources[policy.NSources-1].ListID != frontend.Variable(uint8(policy.NSources)) {
		t.Fatalf("the PrivateFrozen slot reads list %v", wires.Sources[policy.NSources-1].ListID)
	}
}

func assertAllZeroVariables(t *testing.T, v reflect.Value, path string) {
	t.Helper()
	varType := reflect.TypeFor[frontend.Variable]()
	switch {
	case v.Type() == varType:
		if v.IsNil() {
			t.Fatalf("%s is nil", path)
		}
		if v.Interface() != 0 {
			t.Fatalf("%s = %v, want 0", path, v.Interface())
		}
	case v.Kind() == reflect.Struct:
		for i := range v.NumField() {
			f := v.Type().Field(i)
			if f.Tag.Get("gnark") == "-" {
				continue
			}
			assertAllZeroVariables(t, v.Field(i), path+"."+f.Name)
		}
	case v.Kind() == reflect.Array || v.Kind() == reflect.Slice:
		for i := range v.Len() {
			assertAllZeroVariables(t, v.Index(i), path)
		}
	}
}

func TestPrivateWriteRequiresOwner(t *testing.T) {
	private := requestOf[PrivateApplyRecordsParams](t, "custom-ring-apply-records-private-2", ringregistry.PrivateBatch(t, 2, ringregistry.PrivateBlockBit))
	private.Owners[0] = nil
	if _, err := decode("custom-ring-apply-records-private-2", mustMarshal(t, private)); err == nil || !strings.Contains(err.Error(), "missing private owner") {
		t.Fatalf("missing private owner: %v", err)
	}
}

func TestAssignNilSlotsAndAbsentPrivateOwnerRoundTrip(t *testing.T) {
	records := applyRequest(t, "custom-ring-apply-records-2", 2)
	records.Count = 1
	records.Writes[1] = nil
	assigned, err := decode("custom-ring-apply-records-2", mustMarshal(t, records))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := frontend.NewWitness(assigned, ecc.BN254.ScalarField()); err != nil {
		t.Fatal(err)
	}
	assertAllZeroVariables(t, reflect.ValueOf(assigned.(*registry.ApplyRecordsCircuit).Writes[1]), "Writes[1]")

	batch := ringregistry.PrivateBatch(t, 2, ringregistry.PrivateBlockBit)
	batch.Owners[0].Present = 0
	batch.Owners[0].Ring = 1
	batch.Owners[0].Owner = 42
	private := requestOf[PrivateApplyRecordsParams](t, "custom-ring-apply-records-private-2", batch)
	if private.Owners[0].Absent == nil || !private.Owners[0].Ring {
		t.Fatalf("absent ring owner dropped in requestOf: %+v", private.Owners[0])
	}
	private.Writes[1] = nil
	private.Owners[1] = nil
	assigned, err = decode("custom-ring-apply-records-private-2", mustMarshal(t, private))
	if err != nil {
		t.Fatal(err)
	}
	if _, err := frontend.NewWitness(assigned, ecc.BN254.ScalarField()); err != nil {
		t.Fatal(err)
	}
	pc := assigned.(*registry.PrivateApplyRecordsCircuit)
	assertAllZeroVariables(t, reflect.ValueOf(pc.Writes[1]), "Private.Writes[1]")
	assertAllZeroVariables(t, reflect.ValueOf(pc.Owners[1]), "Private.Owners[1]")
	if pc.Owners[0].Present != frontend.Variable(0) || pc.Owners[0].Ring != frontend.Variable(1) || fmt.Sprint(pc.Owners[0].Owner) != "42" {
		t.Fatalf("unexpected absent owner assignment: %+v", pc.Owners[0])
	}
}
