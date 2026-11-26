package audit_test

import (
	"bytes"
	"encoding/hex"
	"math/big"
	"slices"
	"testing"

	"github.com/helius-labs/zolana-custom-rings/prover/circuits/audit"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest"
	"github.com/helius-labs/zolana-custom-rings/prover/ringtest/ringaudit"
	"github.com/helius-labs/zolana/prover/server/prover-test/spp/spptest"
)

func TestRealInputCountIsSealed(t *testing.T) {
	c := compileSection(t, plainShape)
	s := ringaudit.NewSection(t, plainShape).Spending(t, 2)
	s.InputCount = 2
	c.RequireSolved(t, s.Circuit(t))
	realInputs, _ := s.Reader().Envelope(t, s.Stream(t))
	if realInputs != 2 {
		t.Fatalf("the reader read R = %d", realInputs)
	}
	for name, count := range map[string]byte{"one less": 1, "one more": 3} {
		t.Run(name, func(t *testing.T) {
			refuseSealed(t, s, func(p *ringaudit.Plaintexts) { p.Envelope[audit.EnvelopeKeyLen] = count })
		})
	}
}

func TestNoSpendEqualsAZeroCount(t *testing.T) {
	c := compileSection(t, plainShape)
	padded := ringaudit.NewSection(t, plainShape).Spending(t, 0)
	c.RequireSolved(t, padded.Circuit(t))
	if !padded.Claim().NoSpend {
		t.Fatal("no live input leaves no_spend clear")
	}
	if realInputs, _ := padded.Reader().Envelope(t, padded.Stream(t)); realInputs != 0 {
		t.Fatalf("the reader read R = %d", realInputs)
	}
	t.Run("no_spend with a live input", func(t *testing.T) {
		s := ringaudit.NewSection(t, plainShape)
		claim := s.Claim()
		claim.NoSpend = true
		c.RefuseStatement(t, s.Circuit(t), s.StatementOf(t, s.Stream(t), claim))
	})
	t.Run("no_spend clear with no live input", func(t *testing.T) {
		claim := padded.Claim()
		claim.NoSpend = false
		c.RefuseStatement(t, padded.Circuit(t), padded.StatementOf(t, padded.Stream(t), claim))
	})
}

func TestDefaultInputNeedsEntryCapacity(t *testing.T) {
	c := compileSection(t, plainShape)
	s := ringaudit.NewSection(t, plainShape)
	s.Inputs[0].Utxo.RingProgramID = big.NewInt(0)
	c.Refuse(t, s.Circuit(t))
}

func TestDefaultBitsAndRecordsFollowTheSlots(t *testing.T) {
	c := compileSection(t, entryShape)
	s := ringaudit.NewSection(t, entryShape)
	realInputs, bits := s.Reader().Envelope(t, s.Stream(t))
	if realInputs != 2 || bits != 0b10 {
		t.Fatalf("the reader read R = %d, default bits %b", realInputs, bits)
	}
	head := s.Shape.Ranges().EnvelopeHeadLen() - audit.DefaultInputBitsLen
	records := head + audit.DefaultInputBitsLen
	entry := records + audit.EntryRecordLen
	ringCommitment := spptest.MustFieldBytes(t, s.Inputs[0].Utxo.Hash(t))
	for name, edit := range map[string]func(*ringaudit.Plaintexts){
		"default bit cleared":        func(p *ringaudit.Plaintexts) { p.Envelope[head+7] = 0 },
		"default bit on a ring note": func(p *ringaudit.Plaintexts) { p.Envelope[head+7] = 0b11 },
		"record on a ring input":     func(p *ringaudit.Plaintexts) { p.Envelope[records] = 1 },
		"another owner key byte":     func(p *ringaudit.Plaintexts) { p.Envelope[entry] ^= 1 },
		"another nullifier key":      func(p *ringaudit.Plaintexts) { p.Envelope[entry+63] ^= 1 },
		"another amount":             func(p *ringaudit.Plaintexts) { p.Envelope[entry+71] ^= 1 },
		"another mint byte":          func(p *ringaudit.Plaintexts) { p.Envelope[entry+103] ^= 1 },
		"another commitment":         func(p *ringaudit.Plaintexts) { p.Envelope[entry+135] ^= 1 },
		"the ring input's commitment": func(p *ringaudit.Plaintexts) {
			copy(p.Envelope[entry+104:entry+audit.EntryRecordLen], ringCommitment[:])
		},
		"records and bits swapped to input 0": func(p *ringaudit.Plaintexts) {
			first := bytes.Clone(p.Envelope[records:entry])
			copy(p.Envelope[records:entry], p.Envelope[entry:entry+audit.EntryRecordLen])
			copy(p.Envelope[entry:entry+audit.EntryRecordLen], first)
			p.Envelope[head+7] = 0b01
		},
	} {
		t.Run(name, func(t *testing.T) { refuseSealed(t, s, edit) })
	}
	for name, mutate := range map[string]func(*ringaudit.Audit){
		"record naming another owner": func(s *ringaudit.Audit) { s.Inputs[1].OwnerKey[0] ^= 1 },
		"record naming another mint":  func(s *ringaudit.Audit) { s.Inputs[1].Mint[0] ^= 1 },
	} {
		t.Run(name, func(t *testing.T) {
			s := ringaudit.NewSection(t, entryShape)
			mutate(&s)
			c.Refuse(t, s.Circuit(t))
		})
	}
}

// A resend repeats the sealing key, the salt alone keeps its keystream from repeating.
func TestResendUnderAnotherSaltSealsAnotherStream(t *testing.T) {
	s := ringaudit.NewSection(t, plainShape)
	first := s.Stream(t)
	s.Salt[0] ^= 1
	if bytes.Equal(first.Ciphertext, s.Stream(t).Ciphertext) {
		t.Fatal("two salts sealed one stream")
	}
	t.Run("a stream sealed under another salt fails the binding", func(t *testing.T) {
		honest, resent := ringaudit.NewSection(t, plainShape), ringaudit.NewSection(t, plainShape)
		resent.Salt[0] ^= 1
		compileSection(t, plainShape).RefuseStatement(t, honest.Circuit(t), honest.StatementOf(t, resent.Stream(t), honest.Claim()))
	})
}

func TestEntryRecordsServeMixedOwners(t *testing.T) {
	c := compileSection(t, entryShape)
	s := ringaudit.NewSection(t, entryShape)
	other := ringtest.Repeat(0xa3)
	s.Inputs[0] = ringaudit.Input{Utxo: ringaudit.SpentUtxo(t, ringtest.MustSolanaPkField(t, other), spptest.MustNullifierPk(t, big.NewInt(9)), big.NewInt(0), 30, 53), OwnerKey: other, Mint: ringaudit.FixtureMint}
	c.RequireSolved(t, s.Circuit(t))
	plaintext := s.Keys.Open(t, s.Stream(t))
	head := s.Shape.Ranges().EnvelopeHeadLen()
	if !bytes.Equal(plaintext[head:head+32], other[:]) || !bytes.Equal(plaintext[head+audit.EntryRecordLen:head+audit.EntryRecordLen+32], ringaudit.DefaultOwnerKey[:]) {
		t.Fatal("an entry record names another owner")
	}
}

// A clear layout bit holds that hash to zero on every slot, and the bits are the frame's.
func TestDataHashesFollowTheLayoutBits(t *testing.T) {
	c := compileSection(t, plainShape)
	c.RequireSolved(t, dataSection(t).Circuit(t))
	const ring, exit = 0, 2
	for name, mutate := range map[string]func(*ringaudit.Audit){
		"a data hash under a clear data bit":           func(s *ringaudit.Audit) { s.Outputs[exit].Utxo.DataHash, s.Data = big.NewInt(0), false },
		"an exit's data hash under a clear data bit":   func(s *ringaudit.Audit) { s.Outputs[ring].Utxo.DataHash, s.Data = big.NewInt(0), false },
		"a ring data hash under a clear ring data bit": func(s *ringaudit.Audit) { s.RingData = false },
	} {
		t.Run(name, func(t *testing.T) {
			s := dataSection(t)
			mutate(&s)
			c.Refuse(t, s.Circuit(t))
		})
	}
	for name, bit := range map[string]int64{"a data bit": ringaudit.LayoutData, "a ring data bit": ringaudit.LayoutRingData} {
		t.Run(name+" the frame clears against the proof", func(t *testing.T) {
			s := dataSection(t)
			statement := s.Statement(t)
			const layout = 9
			statement[layout] = new(big.Int).Xor(statement[layout], big.NewInt(bit))
			c.RefuseStatement(t, s.Circuit(t), statement)
		})
		t.Run(name+" the frame sets against the proof", func(t *testing.T) {
			s := ringaudit.NewSection(t, plainShape)
			statement := s.Statement(t)
			const layout = 9
			statement[layout] = new(big.Int).Xor(statement[layout], big.NewInt(bit))
			c.RefuseStatement(t, s.Circuit(t), statement)
		})
	}
}

func TestLayoutIsTheKeys(t *testing.T) {
	refuse := func(t *testing.T, shape audit.StreamShape, layout int) {
		t.Helper()
		s := ringaudit.NewSection(t, shape)
		circuit := s.Circuit(t)
		circuit.Layout = layout
		compileSection(t, shape).Refuse(t, circuit)
	}
	t.Run("entry on a key with no entry capacity", func(t *testing.T) { refuse(t, plainShape, ringaudit.LayoutEntry) })
	t.Run("no entry on an entry key", func(t *testing.T) { refuse(t, entryShape, 0) })
	t.Run("a bit past the flags", func(t *testing.T) { refuse(t, plainShape, 1<<4) })
}

// The auditor opens the voucher's key from its own blocks, and a frame nobody vouches for seals zeros there.
func TestStreamSealsTheVoucherKey(t *testing.T) {
	for _, shape := range []audit.StreamShape{plainShape, entryShape} {
		c := compileSection(t, shape)
		region := func(s ringaudit.Audit) []byte {
			return s.Keys.Open(t, s.Stream(t))[shape.Ranges().VoucherOffset():][:audit.KeyLen]
		}
		vouched := ringaudit.NewSection(t, shape)
		vouched.Voucher = ringaudit.VoucherKey
		c.RequireSolved(t, vouched.Circuit(t))
		if !bytes.Equal(region(vouched), vouched.SignerKey()) {
			t.Fatal("the stream seals another key")
		}
		if !bytes.Equal(region(ringaudit.NewSection(t, shape)), make([]byte, audit.KeyLen)) {
			t.Fatal("an unvouched stream seals a key")
		}
		t.Run("a voucher region the frame left out", func(t *testing.T) {
			unvouched := vouched
			unvouched.Voucher = nil
			c.RefuseStatement(t, vouched.Circuit(t), unvouched.Statement(t))
		})
		t.Run("a flipped voucher ciphertext byte", func(t *testing.T) {
			stream := vouched.Stream(t)
			stream.Voucher()[0] ^= 1
			c.RefuseStatement(t, vouched.Circuit(t), vouched.StatementOf(t, stream, vouched.Claim()))
		})
	}
}

// The reader rebuilds every slot from its record alone, with no mint registry and no payload.
func TestReaderRebuildsEverySlot(t *testing.T) {
	for name, s := range map[string]ringaudit.Audit{
		"plain": ringaudit.NewSection(t, plainShape), "entry": ringaudit.NewSection(t, entryShape), "data": dataSection(t),
	} {
		stream, hashes, reader := s.Stream(t), s.PublishedHashes(t), s.Reader()
		for i, hash := range hashes {
			note, err := reader.Read(t, stream, i, hash)
			if err != nil {
				t.Fatalf("%s slot %d: %v", name, i, err)
			}
			utxo := s.Outputs[i].Utxo
			if !utxo.IsUtxo() {
				if note != nil {
					t.Fatalf("%s slot %d: a dummy read as a note", name, i)
				}
				continue
			}
			if note == nil || note.Hash.Cmp(hash) != 0 || note.Blinding.Cmp(utxo.Blinding) != 0 || note.DataHash.Cmp(utxo.DataHash) != 0 || note.RingDataHash.Cmp(utxo.RingDataHash) != 0 {
				t.Fatalf("%s slot %d: read %+v", name, i, note)
			}
		}
	}
	t.Run("a reader of another layout rebuilds no data note", func(t *testing.T) {
		s := dataSection(t)
		reader := s.Reader()
		reader.Data = false
		if note, err := reader.Read(t, s.Stream(t), 0, s.Outputs[0].Utxo.Hash(t)); err == nil {
			t.Fatalf("read %+v", note)
		}
	})
}

// The auditor never saw a default-ring note, its record names the note and the commitment SPP spends.
func TestReaderReadsADefaultInput(t *testing.T) {
	s := ringaudit.NewSection(t, entryShape)
	s.Inputs[1].Utxo.DataHash = big.NewInt(0xda7a)
	s.Inputs[1].Utxo.RingDataHash = big.NewInt(0x5ca1e)
	compileSection(t, entryShape).RequireSolved(t, s.Circuit(t))
	stream, reader, spent := s.Stream(t), s.Reader(), s.Inputs[1].Utxo
	entry, err := reader.ReadEntry(t, stream, 1, spent.Hash(t))
	if err != nil || entry == nil {
		t.Fatalf("read %+v, %v", entry, err)
	}
	if entry.OwnerKey != ringaudit.DefaultOwnerKey || entry.NullifierPk.Cmp(spent.NullifierPk) != 0 || entry.Amount != spent.Amount || entry.Mint != ringaudit.FixtureMint {
		t.Fatalf("read %+v", entry)
	}
	if entry, err := reader.ReadEntry(t, stream, 0, s.Inputs[0].Utxo.Hash(t)); err != nil || entry != nil {
		t.Fatalf("a ring input read as an entry %+v, %v", entry, err)
	}
	if _, err := reader.ReadEntry(t, stream, 1, big.NewInt(0xdead)); err == nil {
		t.Fatal("the reader accepted a commitment the record does not name")
	}
}

// The record names each slot's key, the reader tries none.
func TestZeroAndEnrolledKeysReadApart(t *testing.T) {
	s := ringaudit.NewSection(t, plainShape)
	stream, hashes, reader := s.Stream(t), s.PublishedHashes(t), s.Reader()
	enrolled, err := reader.Read(t, stream, 0, hashes[0])
	if err != nil {
		t.Fatal(err)
	}
	zero, err := reader.Read(t, stream, 1, hashes[1])
	if err != nil {
		t.Fatal(err)
	}
	if enrolled.NullifierPk.Cmp(ringaudit.EnrolledNullifierPk(t)) != 0 || zero.NullifierPk.Cmp(ringaudit.ZeroKey(t)) != 0 {
		t.Fatalf("read keys %x and %x", enrolled.NullifierPk, zero.NullifierPk)
	}
}

// The slot index enters the slot secret, a record moved to another slot decrypts to noise.
func TestRecordAtAnotherSlotFailsTheReader(t *testing.T) {
	s := ringaudit.NewSection(t, plainShape)
	stream := s.Stream(t)
	moved := stream
	moved.Records = slices.Clone(stream.Records)
	moved.Records[1] = stream.Record(0)
	if note, err := s.Reader().Read(t, moved, 1, s.Outputs[0].Utxo.Hash(t)); err == nil {
		t.Fatalf("a moved record read as %+v", note)
	}
	compileSection(t, plainShape).RefuseStatement(t, s.Circuit(t), s.StatementOf(t, moved, s.Claim()))
}

func TestStreamVector(t *testing.T) {
	type vectorCase struct {
		Name          string   `json:"name"`
		Layout        int64    `json:"layout"`
		InputClass    int      `json:"input_class"`
		Outputs       int      `json:"outputs"`
		OutputCount   int      `json:"output_count"`
		InputCount    int      `json:"input_count"`
		Envelope      string   `json:"envelope_plaintext"`
		EnvelopeCt    string   `json:"envelope_ciphertext"`
		RecipientKeys []string `json:"recipient_keys"`
		KeysCt        string   `json:"recipient_keys_ciphertext"`
		Voucher       string   `json:"voucher"`
		Records       []string `json:"records"`
		RecordCts     []string `json:"record_ciphertexts"`
		OutputHashes  []string `json:"output_hashes"`
		Blindings     []string `json:"blindings"`
		EnvelopeHash  string   `json:"envelope_digest"`
		RecordChain   string   `json:"record_chain"`
	}
	data := dataSection(t)
	defaultFirst := ringaudit.NewSection(t, audit.StreamShape{InputClass: 4, Outputs: 2, Entry: true})
	defaultFirst.Inputs[0], defaultFirst.Inputs[1] = defaultFirst.Inputs[1], defaultFirst.Inputs[0]
	vouched := ringaudit.NewSection(t, plainShape)
	vouched.Voucher = ringaudit.VoucherKey
	sections := []struct {
		name    string
		section ringaudit.Audit
	}{
		{"plain_with_exit_and_dummy", ringaudit.NewSection(t, plainShape)},
		{"compact", ringaudit.NewSection(t, plainShape).Omit(t, 1)},
		{"data", data},
		{"entry", ringaudit.NewSection(t, entryShape)},
		{"no_spend", ringaudit.NewSection(t, plainShape).Spending(t, 0)},
		{"entry_default_first", defaultFirst},
		{"vouched", vouched},
	}
	cases := make([]vectorCase, len(sections))
	for k, tc := range sections {
		s := tc.section
		compileSection(t, s.Shape).RequireSolved(t, s.Circuit(t))
		p, stream := s.Plaintexts(t), s.Stream(t)
		v := vectorCase{
			Name: tc.name, Layout: s.Layout(), InputClass: s.Shape.InputClass, Outputs: s.Shape.Outputs, OutputCount: s.OutputCount(), InputCount: s.InputCount,
			Envelope: hex.EncodeToString(p.Envelope), EnvelopeCt: hex.EncodeToString(stream.Envelope(s.InputCount)),
			KeysCt:       hex.EncodeToString(sealedKeys(s, stream)),
			EnvelopeHash: ringtest.FieldHex(s.StatementOf(t, stream, s.Claim())[7]),
			RecordChain:  ringtest.FieldHex(ringaudit.RecordChain(t, s.StatementOutputs(t, stream))),
		}
		for i := range s.OutputCount() {
			v.RecipientKeys = append(v.RecipientKeys, hex.EncodeToString(p.RecipientKeys[i][:]))
			v.Records = append(v.Records, hex.EncodeToString(p.Records[i][:s.PublishedLen(i)]))
			v.RecordCts = append(v.RecordCts, hex.EncodeToString(stream.Record(i)))
			v.OutputHashes = append(v.OutputHashes, ringtest.FieldHex(s.Outputs[i].Utxo.Hash(t)))
			v.Blindings = append(v.Blindings, ringtest.FieldHex(s.Blinding(t, i)))
		}
		if s.Vouched() {
			v.Voucher = hex.EncodeToString(s.SignerKey())
		}
		cases[k] = v
	}
	txViewingSk, auditorSk := sections[0].section.Keys.TxViewingSk(), ringaudit.AuditorSk()
	ringtest.PinVector(t, "audit/audit-stream.json", map[string]any{
		"description":     "Auditor streams under CRING/adt and output records under CRING/rcp and each slot's delivered secret, from solved witnesses. Records and record_ciphertexts hold the published prefix of the 169 byte record, 105 bytes and 32 per layout flag on a ring slot or a dummy, 137 bytes and 32 under the data flag on an exit. Each record_chain cell is Poseidon(exit, hash_bytes(record_ciphertext)). recipient_keys_ciphertext is the sealed key region a frame carries, the slot keys then the voucher key on a vouched frame, and voucher is that key's plaintext.",
		"tx_viewing_sk":   hex.EncodeToString(txViewingSk[:]),
		"auditor_sk":      hex.EncodeToString(auditorSk[:]),
		"salt":            hex.EncodeToString(sections[0].section.Salt[:]),
		"first_nullifier": ringtest.FieldHex(sections[0].section.FirstNullifier),
		"ring_id":         ringtest.FieldHex(sections[0].section.RingProgramID),
		"output_tree_id":  ringaudit.OutputTreeID,
		"cases":           cases,
	})
}

// sealedKeys is the key region a frame carries, the slot keys then the voucher's on a vouched frame.
func sealedKeys(s ringaudit.Audit, stream ringaudit.Stream) []byte {
	keys := stream.Keys(s.OutputCount())
	if s.Vouched() {
		return slices.Concat(keys, stream.Voucher())
	}
	return keys
}
