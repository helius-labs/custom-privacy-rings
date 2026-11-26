// Fixes where each byte of the auditor stream and of each output record sits, per key shape and per frame.
//
// A circuit encrypts one plaintext length per key, and a frame publishes less. The key alone fixes
// every counter, no published count moves a region, and a reader decrypts recipient key i at its own
// counter with no other slot. The frame omits padding and unpublished slots, so the bytes the statement
// hashes are the regions the published n and m select.
//
// The auditor stream holds the envelope, then one recipient key per output slot at full output capacity,
// then the voucher's key. The voucher region sits last, so no key's counter moves and a frame that is not
// vouched omits it like an unpublished slot.
// The envelope seals the viewing key and the note count before any entry evidence. Entry evidence keeps
// one record per published input in note order. Key counters start after the envelope at full input
// capacity, rounded up to an AES block. Readers must use that capacity even when the frame carries fewer
// inputs, or they select another keystream.
//
// Each output record is its own stream under its slot secret, so no counter of the auditor stream serves it.
// Every slot seals TransferRecordLen bytes and publishes a prefix. The frame's data flags and the slot's
// class fix the prefix, so one length serves every ring slot and dummy of a frame and no length names a data note.
//
// The file proves the following.
//  1. The layout value holds the vouched, entry, data and ring data flags, a value past them fails its decomposition.
//  2. The entry flag equals the key's entry capacity.
//  3. Range construction rejects an input capacity outside the entry bitmap.
//  4. A clear data flag holds every slot's data hash to zero, a clear ring data flag every ring data hash,
//     and an exit carries no ring data hash.
//
// Read after shared/envelope.go. Read before audit/audit.go, audit/record.go.

package audit

import (
	"github.com/consensys/gnark/frontend"
	"github.com/reilabs/gnark-lean-extractor/v3/abstractor"

	"github.com/helius-labs/zolana-custom-rings/prover/circuits/shared"
	"github.com/helius-labs/zolana/prover/server/circuits/gadget"
)

const layoutBits = 4

// LayoutFlags are the layout bits a frame chooses.
type LayoutFlags struct {
	Vouched, Data, RingData frontend.Variable
}

type StreamShape struct {
	InputClass, Outputs int
	Entry               bool
}

// AssertLayout pins the layout to the key's entry flag and returns the flags a frame chooses.
func (s StreamShape) AssertLayout(api frontend.API, layout frontend.Variable) LayoutFlags {
	// The decomposition bounds the layout to its four bits.
	bits := api.ToBinary(layout, layoutBits)
	entry := 0
	if s.Entry {
		entry = 1
	}
	api.AssertIsEqual(bits[1], entry)
	return LayoutFlags{Vouched: bits[0], Data: bits[2], RingData: bits[3]}
}

// assertHashes holds a hash under a clear flag to zero, the reader rebuilds the commitment with zero there.
func (l LayoutFlags) assertHashes(api frontend.API, slot shared.Output) {
	abstractor.CallVoid(api, gadget.AssertZeroWhen{Cond: api.Sub(1, l.Data), V: slot.DataHash})
	abstractor.CallVoid(api, gadget.AssertZeroWhen{Cond: api.Sub(1, l.RingData), V: slot.RingDataHash})
	// An exit publishes its nullifier key where a ring slot publishes ring data.
	abstractor.CallVoid(api, gadget.AssertZeroWhen{Cond: slot.Exit, V: slot.RingDataHash})
}

// StreamRanges mirrors AuditStreamLayout in program-libs/interface/src/proof/audit_stream.rs.
//
//	envelope, EnvelopeBlocks | recipient keys, Outputs * KeyBlocks | voucher key, KeyBlocks
type StreamRanges struct {
	StreamShape
}

const (
	streamBlockLen = 16
	// StreamFirstCounter is aes.Cipher.CTREncrypt's first counter, mirrored by STREAM_FIRST_COUNTER.
	StreamFirstCounter  = 2
	EnvelopeKeyLen      = 32
	RealInputCountLen   = 1
	DefaultInputBitsLen = 8
	fieldLen            = 32
	amountLen           = 8
	kindLen             = 1
	// TransferRecordLen is the plaintext every transfer slot seals, mirrored by TRANSFER_RECORD_LEN.
	TransferRecordLen = RecordLen + fieldLen
	// RecordLen is the head and the nullifier key, the prefix an exit publishes with no data hash and the record a merge seals before its seed.
	RecordLen = RingRecordLen + fieldLen
	// RingRecordLen is the head every slot publishes, kind to blinding.
	RingRecordLen = kindLen + amountLen + 3*fieldLen
	// KeyLen is one sealed recipient key, its region holds no padding.
	KeyLen    = fieldLen
	KeyBlocks = KeyLen / streamBlockLen
	// EntryRecordLen mirrors ENTRY_RECORD_LEN.
	EntryRecordLen = amountLen + 4*fieldLen
)

func (s StreamShape) Ranges() StreamRanges {
	if err := shared.ValidateInputCapacity(s.InputClass); err != nil {
		panic(err)
	}
	return StreamRanges{StreamShape: s}
}

func (r StreamRanges) EnvelopeHeadLen() int {
	head := EnvelopeKeyLen + RealInputCountLen
	if r.Entry {
		return head + DefaultInputBitsLen
	}
	return head
}

func (r StreamRanges) entryLen() int {
	if r.Entry {
		return EntryRecordLen
	}
	return 0
}

// EnvelopeLen is what the frame carries, with no padding.
func (r StreamRanges) EnvelopeLen(count int) int {
	return r.EnvelopeHeadLen() + count*r.entryLen()
}

// EnvelopeBlocks is fixed per shape, no published count moves a key's counter.
func (r StreamRanges) EnvelopeBlocks() int {
	return (r.EnvelopeLen(r.InputClass) + streamBlockLen - 1) / streamBlockLen
}

// KeyCounter must match the readers, another counter decrypts noise.
func (r StreamRanges) KeyCounter(i int) int {
	return StreamFirstCounter + r.EnvelopeBlocks() + i*KeyBlocks
}

// CapacityBlocks covers the envelope, every slot's key and the voucher's key, a frame omits what it does not publish.
func (r StreamRanges) CapacityBlocks() int {
	return r.EnvelopeBlocks() + (r.Outputs+1)*KeyBlocks
}

// VoucherOffset is the block past the last slot's key.
func (r StreamRanges) VoucherOffset() int {
	return r.KeyOffset(r.Outputs)
}

func (r StreamRanges) KeyOffset(i int) int {
	return (r.KeyCounter(i) - StreamFirstCounter) * streamBlockLen
}
