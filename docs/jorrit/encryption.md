# Encryption

## Current encoding sizes

These sizes count the encryption fields and discovery tags carried by the compact ring transfer instruction. They assume ring outputs only, no default-ring inputs, and no application payloads, replies or extra messages.

`M` is the number of published output slots, including recipient payments, change and dummies. Compact padding omitted from the instruction adds no bytes.

| Component | Confidential | Anonymous |
|---|---:|---:|
| Transaction viewing public key | 32 B | 32 B |
| Salt | 16 B | 16 B |
| Encrypted auditor envelope: viewing secret and spent-note count | 33 B | 33 B |
| Encrypted recipient viewing keys for the auditor | `32 × M` B | `32 × M` B |
| Encrypted output records | `105 × M` B | `105 × M` B |
| Output discovery tags | `8 × M` B | `8 × M` B |
| Encrypted sender bundle | — | 32 B |
| Sender bundle discovery tag | — | 32 B |
| **Total** | **`81 + 145 × M` B** | **`145 + 145 × M` B** |

Each 105-byte ring output record contains its kind (1 B), amount (8 B), owner key (32 B), mint (32 B) and blinding (32 B). The nullifier public key is omitted from a ring record; the reader uses the owner's registration or the record's zero-key flag. The recipient and auditor open the same record, so there is one encrypted copy per output.

| Published output slots (`M`) | Confidential bytes | Anonymous bytes |
|---|---:|---:|
| 1 | 226 | 290 |
| 2 | 371 | 435 |
| 4 | 661 | 725 |
| 8 | 1,241 | 1,305 |
| 16 | 2,401 | 2,465 |

Counting ciphertext alone gives `33 + 137 × M` bytes for confidential transfers and `65 + 137 × M` for anonymous transfers. The totals above also include the public viewing key, salt and discovery tags. AES-CTR adds no authentication tag or transmitted block padding.

The complete instruction additionally carries output commitments (`32 × M` B), proofs, nullifiers, root revisions, counts, the exit bitmap and authorization fields. Transaction signatures, account addresses and the message header add further bytes. A row in this table therefore does not establish that the complete transaction fits.

### Additional fields

- **Default-ring entry:** the auditor envelope grows from 33 B to `41 + 136 × N` B, adding `8 + 136 × N` bytes. `N` is the published input/nullifier count, including padding and address inputs, rather than just the number of default-ring notes. The instruction can require additional public authorization fields too.
- **Application payloads:** with `HAS_DATA`, each output gets a 2-byte body-length field. Each non-exit output also carries the frame's shared encrypted payload width, including its internal encoding and padding.
- **Replies:** add a 2-byte shared length and `L` bytes per non-exit output, where `L >= 97`. Slots without a reply carry filler of the same length.
- **Vouched transfer:** the auditor stream carries the voucher's key, adding 32 B.
- **Default-ring exits:** each exit carries a 137-byte audit record, a full 32-byte owner tag and a separate SPP recipient body. The table's ring-output formula does not cover exits.

Sources: [audit stream lengths](../../program-libs/interface/src/proof/audit_stream.rs), [compact transfer encoder](../../program-libs/interface/src/instructions/transact.rs), [output body encoding](../../program-libs/interface/src/codec/output.rs). Complete transaction profiles are measured separately in [ring-frame-fit.md](../../test-vectors/proof/ring-frame-fit.md).

## Earlier plaintext-layout draft

The structs below describe an earlier layout and encryption guarantees. The current size tables above follow the implemented compact encoding.

```rust
// Earlier custom-ring layout: standard anonymous transfers, merges and key registration.
// Logical plaintext layouts; Vec/Option grouping is not the wire encoding.
// Layout reference: https://github.com/helius-labs/zolana/blob/alex/anonymous-layout-e2e/docs/spec.md#anonymous
//
// VERIFIED: the circuit proves that the ciphertext encrypts these values to the configured auditor.
// UNVERIFIED: ciphertext bytes are committed, but correct encryption, plaintext and delivery keys
// are not proven. Recipients validate decrypted UTXOs by comparing their reconstructed hashes.
// The linked spec's "verified" recipient/sender fields refer to the auditor's proven disclosure;
// this does not make the recipient/sender ciphertexts verifiably encrypted.
//
// Transaction viewing public key, salt, output hashes and discovery tags are public, not encrypted.
// Recipient/sender discovery tags are unproven hints; SPP separately binds exit owner tags.

type Field = [u8; 32];
type Pubkey = [u8; 32];
type ViewingPublicKey = [u8; 32]; // X25519 u-coordinate; scheme tag omitted here.
type EncodedData = Vec<u8>;     // Serialized application/ring data.

struct AnonymousTransferPlaintexts {
    verified: VerifiedAuditorMessagePlaintext, // One AES-256-CTR stream to the auditor.
    unverified: UnverifiedDeliveryPlaintexts,  // Separate recipient/sender ciphertexts.
}

// VERIFIED ENCRYPTION: transfer proof checks X25519, the Poseidon KDF, AES and the ciphertext hashes.
struct VerifiedAuditorMessagePlaintext {
    envelope: VerifiedAuditEnvelopePlaintext,
    output_records: Vec<VerifiedOutputRecordPlaintext>, // One per included output, including exits/dummies.
    output_data: Option<Vec<VerifiedOutputDataPlaintext>>, // Present when has_data = 1.
}

struct VerifiedAuditEnvelopePlaintext {
    tx_viewing_secret: [u8; 32], // Also derives every transfer output's blinding.
    spent_utxo_count: u8,
    address_input_bitmap: u64,
    entry: Option<VerifiedEntryPlaintext>, // Only on a default-ring entry key.
}

struct VerifiedEntryPlaintext {
    default_ring_input_bitmap: u64,
    inputs: Vec<VerifiedEntryInputPlaintext>, // One per included input; zero for other input types.
}

struct VerifiedEntryInputPlaintext {
    owner_key: Pubkey,
    nullifier_pk: Field,
    amount: u64,
    mint: Pubkey,
    blinding: Field,
    data_hash: Field,
    ring_data_hash: Field,
}

struct VerifiedOutputRecordPlaintext {
    kind: u8, // 0 = dummy, 1 = ring output, 2 = default-ring exit.
    amount: u64,
    owner_key: Pubkey,
    mint: Pubkey,
    nullifier_pk: Field,
    // Dummy: every plaintext byte is zero; its ciphertext is encrypted normally.
    // Blinding is derived from the envelope's transaction viewing secret, salt and first nullifier.
}

struct VerifiedOutputDataPlaintext {
    data_hash: Field,
    ring_data_hash: Field,
    // Hashes only: the application data itself is not verifiably encrypted.
    // Both fields are zero for dummies. Without this record, both committed hashes must be zero.
}

// UNVERIFIED ENCRYPTION: SPP commits these bytes, but no circuit checks their encrypted contents.
// An audited amount/mint in the verified records does not prove that a delivery copy contains it.
struct UnverifiedDeliveryPlaintexts {
    recipient_bodies: Vec<UnverifiedRecipientPlaintext>, // To each recipient's viewing key.
    sender_bundle: Option<UnverifiedSenderBundlePlaintext>, // To the sender's viewing key.
    replies: Vec<Option<UnverifiedReplyPlaintext>>, // To the recipient, under a separate cipher label.
    exit_bodies: Vec<UnverifiedExitRecipientPlaintext>, // Standard SPP confidential delivery.
    // Bodiless outputs have no recipient ciphertext. Dummy bodies/replies contain random bytes.
}

struct UnverifiedRecipientPlaintext {
    blinding: Field,
    amount: u64,
    mint: Pubkey,
    ring_data_hash: Option<Field>,
    data: Vec<u8>, // When nonempty: data_hash followed by application data.
}

struct UnverifiedSenderBundlePlaintext {
    output_blinding_seed: Field,
    change: Vec<UnverifiedChangePlaintext>, // In bodiless output-slot order.
    recipient_viewing_keys: Vec<ViewingPublicKey>, // One per body-bearing slot; zero for exits/dummies.
}

struct UnverifiedChangePlaintext {
    mint: Pubkey,
    amount: u64,
}

struct UnverifiedReplyPlaintext {
    claimed_sender_viewing_key: ViewingPublicKey,
    claimed_sender_owner_key: Pubkey,
    claimed_sender_nullifier_pk: Field,
    data: EncodedData,
    // Every field is an unauthenticated sender claim.
}

struct UnverifiedExitRecipientPlaintext {
    asset_id: u64,
    amount: u64,
    blinding: Field,
    ring_program_id: Option<Pubkey>, // None for a default-ring exit.
    data: EncodedData,
}

// VERIFIED ENCRYPTION in other instructions: separate X25519/Poseidon-KDF/AES-256-CTR ciphertexts.
struct VerifiedMergeAuditorPlaintext {
    output: VerifiedOutputRecordPlaintext, // kind = 1; bound to the merged output hash.
    merge_seed: Field, // Derives the merged blinding; equals the nullifier secret for nonzero-secret notes.
}

struct VerifiedKeyRegistrationAuditorPlaintext {
    nullifier_secret: Field, // Nonzero, with a zero leading byte; hashes to the registered nullifier_pk.
    list_salt: Field,        // Nonzero, with a zero leading byte; hashes to the registered list_tag.
}
```
