# Ring keys crate test invariants

Coverage specification for `zolana-ring-keys`, the slot secrets, view tags, ring stream,
record codecs, list salt, ring address and rotating key holder of a custom ring. It follows
the [shielded-pool invariant checklist](../../../zolana/program-tests/shielded-pool/invariants/README.md)
and holds Critical and High rows only. Rust tests implement its claims. The TypeScript twin of
each row is in [the TypeScript checklist](../../ts/test/INVARIANTS.md). The protocol text
lives in [Disclosure](../../../docs/spec/crypto/disclosure.md),
[Envelope](../../../docs/spec/crypto/envelope.md) and [Hashing](../../../docs/spec/crypto/hashing.md).

## Reading and maintaining the checklist

IDs are stable. Never renumber an existing invariant, and give a new row the next free number
of its area. Each row states one observable requirement, not the steps of the code. The
source line of each area names the implementation. The coverage cell names the test that
asserts the claim.

- `[x]` Covered: a direct assertion exists and fails when the invariant breaks.
- `[ ]` Partial: some assertions exist. The cell states the missing case.
- `[ ]` Missing: no test asserts the claim. The row is a test work item.

Kinds: **pre** is an acceptance or refusal condition, **post** a result on success,
**frame** what an operation leaves unchanged, **wire** a byte layout or derivation another
implementation must reproduce. Severity: **C** is value or spend authority given to the
wrong party, **H** is a leaked secret, a note lost to its owner, or an encoding another
party rejects. Medium and lower rows are out of scope.

A test reference without a directory is a file in this directory. `src/<file>::tests::<name>`
is an inline test of this crate. A path from the repository root names a test of a caller
crate that exercises this crate. Every path names code of this tree. The symbol is the anchor,
and a line number only helps to find it. A vector is evidence only when its provenance is known.
A vector this crate writes pins drift and the TypeScript and Go readers, and it does not
prove the formula. A round trip through a hand codec does not pin a layout, since encode and
decode can move a field together.

To update a row, read the production symbol and the test assertion, add the exact refusal
and the boundary case, run `cargo test -p zolana-ring-keys`, then update the cell.

## Slot secrets and tags

Source: [slot secret](../src/slot_secret.rs) `DeliveredSlot::secret`, `bootstrap_slot_tag`,
[key holder](../src/keys.rs) `delivery_secrets`, `view_tags`, `Agreements::of`,
[dummy key](../src/dummy_key.rs) `dummy_recipient_key`,
[blinding](../src/blinding.rs) `ring_blinding_seed`.

| ID | Kind / severity | Invariant | Coverage / required assertions |
|---|---|---|---|
| INV-KEYS-SLOT-01 | wire / H | For every input, `DeliveredSlot::secret` equals `Poseidon(CRDS, pack_be(dh), pack_be(tx_viewing_pk), pack_be(recipient_viewing_pk), right_align(salt), i, n0)`, the slot secret the Go circuits seal under. | [x] `go_slot_vectors.rs::a_retired_key_answers_the_slot_secret_of_its_body` against the Go vector `test-vectors/audit/anonymous_bodies.json`. `sdk-libs/client/tests/vectors.rs::ring_derivation_vectors` pins three output indexes in `test-vectors/wallet/ring-secrets.json`. |
| INV-KEYS-SLOT-02 | pre / H | `DeliveredSlot::secret` refuses a transaction viewing key or a recipient viewing key whose first byte is not `0x04` with `StreamError::UntaggedKey`. | [ ] Partial: `src/slot_secret.rs::tests::untagged_viewing_keys_are_refused` refuses an untagged recipient key. An untagged transaction viewing key is not tested. |
| INV-KEYS-SLOT-03 | post / H | `ring_delivery_secrets` returns exactly one secret per request, in request order, and each equals the `DeliveredSlot::secret` of that request under the held key it names. | [ ] Partial: `go_slot_vectors.rs::a_retired_key_answers_the_slot_secret_of_its_body` checks two requests under one key and one transaction. No batch interleaves two keys or two transactions and compares each answer with the answer to that request alone, the case the cached agreement serves. |
| INV-KEYS-SLOT-04 | wire / H | For every held key and index `k`, `view_tags` answers `slot_tag(sender_view_tag(k))` on the `Sender` stream and `slot_tag(recipient_request_view_tag(k))` on the `Request` stream, 24 zero bytes then the low 8 bytes of zolana's tag. | [x] `src/keys.rs::tests::each_stream_answers_the_spec_tag_at_the_ring_width`. |
| INV-KEYS-SLOT-05 | post / H | `view_tags` returns one list per request in request order, and each list holds exactly one tag per index of the request's range in index order. | [ ] Partial: `src/keys.rs::tests::each_stream_answers_the_spec_tag_at_the_ring_width` checks one request over indexes 3 and 4. A batch of several requests is not tested. |
| INV-KEYS-SLOT-06 | wire / H | For every viewing key `u`, `bootstrap_slot_tag(u)` equals 24 zero bytes then the last 8 bytes of `u`. | [ ] Missing: the client sync tests take the function's output as their expected tag. No test compares it with an independent value or a shared vector. |
| INV-KEYS-SLOT-07 | wire / H | For every transaction viewing secret and slot `i`, `dummy_recipient_key` equals `X25519(clamp(Poseidon(CRDK, pack_be(tx_viewing_sk), i)), 9)` under RFC 7748 clamping. | [ ] Missing: no test and no shared vector pins the key. `sdk-libs/client/tests/vectors.rs::domains_are_their_ascii_tags` leaves out `CRDK`. |
| INV-KEYS-SLOT-08 | wire / H | `ring_blinding_seed(tx_viewing_secret, salt)` equals `Poseidon(CRBS, pack_be(tx_viewing_secret), right_align(salt))`, the seed of every ring output blinding. | [x] `sdk-libs/client/tests/vectors.rs::ring_derivation_vectors`. `sdk-libs/client/tests/disclosure_vectors.rs::every_stream_rebuilds_its_published_hashes_with_no_mint_registry` rebuilds each output blinding of the Go solved witnesses in `audit-stream.json`. |

## Sealing and opening

Source: [stream cipher](../src/stream_cipher.rs) `RingStream::new`, `RingStream::apply_at`,
`symmetric_apply_audit`, [labels](../src/labels.rs),
[records](../src/audit_records.rs) `OutputRecord::{encode, decode}`, `RecordHead::{read, with_key}`,
`RecordKey::read`, `MergeRecord`, `MergeEnvelope`, `AuditEnvelope::{encode, decode}`,
`decode_recipient_keys`, `RecipientPayload`, `SealedLayout`, `RecipientBodyForms`,
[auditor message](../src/auditor_message.rs) `AuditorMessageRead::parse`,
[output body](../src/output_body.rs) `ring_body_output`, `encrypted_output`.

| ID | Kind / severity | Invariant | Coverage / required assertions |
|---|---|---|---|
| INV-KEYS-SEAL-01 | wire / H | `RingStream::new(secret, label)` keys AES-256-CTR with the low 31 bytes of `head` then byte 19 of `tail`, and takes bytes 20 to 31 of `tail` as the nonce, with `head = Poseidon(right_align(label ‖ 0x00), secret)` and `tail = Poseidon(right_align(label ‖ 0x01), secret)`. | [x] `stream_vectors.rs::stream_vectors_match` derives the key and nonce from the schedule text and compares the ciphertext with the SDK stream. `src/stream_cipher.rs::audit_stream_tests::matches_the_go_auditor_stream_vector` compares with a Go ciphertext. |
| INV-KEYS-SEAL-02 | wire / H | For every `k`, `apply_at(STREAM_FIRST_COUNTER + k)` produces exactly block `k` of the keystream that `symmetric_apply_audit` applies from counter 2, under the IV `nonce ‖ u32be(counter)`. | [x] `src/stream_cipher.rs::audit_stream_tests::a_range_reads_the_same_keystream_as_the_whole_stream`, `matches_the_go_auditor_stream_vector`. |
| INV-KEYS-SEAL-03 | pre / H | Under one secret, no two of the six stream labels give the same keystream. | [ ] Partial: `src/stream_cipher.rs::audit_stream_tests::a_label_separates_the_keystreams_of_one_secret` compares `CRING/adt` with `CRING/nfk` only. |
| INV-KEYS-SEAL-04 | wire / H | Each label constant equals its text in the stream schedule of [Envelope](../../../docs/spec/crypto/envelope.md#stream-schedule): `CRING/adt`, `CRING/rcp`, `CRING/rpl`, `CRING/nfk`, `CRING/mrg` and `CRING/mro`. | [x] `sdk-libs/client/tests/merge.rs::the_merge_record_is_a_transfer_record_and_its_seed` pins `CRING/mrg` and `CRING/mro`. `stream_vectors.rs::stream_vectors_match` pins `CRING/adt` and `CRING/nfk`. `sdk-libs/transaction/tests/go_anonymous_bodies.rs::the_sealer_and_the_reader_agree_with_the_proven_ciphertext` pins `CRING/rcp` against a Go ciphertext. `sdk-libs/transaction/tests/reply_metadata_vector.rs::the_reply_vector_opens_as_pinned` pins the reply ciphertext under `CRING/rpl` in `test-vectors/wallet/reply_metadata.json`. |
| INV-KEYS-SEAL-05 | wire / H | `OutputRecord::encode` writes `u8(kind \| zero key bit) ‖ u64be(amount) ‖ owner_key ‖ mint ‖ blinding` in bytes 0 to 104. After it an exit writes its key then its data hash. A ring output writes its data hash under the data bit and its ring data hash otherwise, then its ring data hash. | [x] `audit_stream.rs::the_record_lays_out_its_contract_bytes`. `audit_stream.rs::the_solved_go_stream_reads_back_under_the_rust_layout` decodes each Go record and encodes it to the same bytes. |
| INV-KEYS-SEAL-06 | pre / H | For every form, `OutputRecord::decode` accepts a ring output or dummy only at `105 + 32 (data + ring_data)` bytes and an exit only at `137 + 32 data` bytes, and refuses every other length with `RingTransactLength`. | [x] `audit_stream.rs::every_form_reads_back_at_its_one_length` refuses one byte less and one byte more for each kind and form. |
| INV-KEYS-SEAL-07 | wire / H | `OutputRecord::encode` sets kind bit 4 exactly when a live record's key is `ZERO_NULLIFIER_PUBKEY`. | [x] `audit_stream.rs::the_zero_key_bit_names_the_zero_key`. |
| INV-KEYS-SEAL-08 | pre / H | `OutputRecord::decode` refuses the zero key bit on a carried key other than `ZERO_NULLIFIER_PUBKEY`, and a carried `ZERO_NULLIFIER_PUBKEY` without the bit. | [x] `audit_stream.rs::a_record_off_its_form_is_refused`. |
| INV-KEYS-SEAL-09 | pre / H | `OutputRecord::decode` refuses every kind byte whose value without bit 4 is not 0, 1 or 2. | [ ] Partial: `audit_stream.rs::a_record_off_its_form_is_refused` refuses kind 3 only. No kind byte with bit 3 or a higher bit is tested. |
| INV-KEYS-SEAL-10 | pre / H | `OutputRecord::decode` returns a dummy only when every published byte is zero. | [x] `audit_stream.rs::a_record_off_its_form_is_refused` refuses a blinding byte, the zero key bit and a hash byte on a dummy. |
| INV-KEYS-SEAL-11 | wire / H | `MergeRecord::encode` writes the record head, then the record key, then the seed, 169 bytes, and `MergeRecord::decode` refuses every other length. | [ ] Partial: `sdk-libs/client/tests/merge.rs::the_merge_record_is_a_transfer_record_and_its_seed` checks the offsets and refuses 168 bytes. `sdk-libs/client/tests/go_vectors.rs::merge_statement_matches_the_go_vector` decodes the Go records. A 170-byte plaintext is not tested. |
| INV-KEYS-SEAL-12 | wire / H | `MergeEnvelope::encode` writes `eph_sk ‖ recipient_u`, 64 bytes, and `MergeEnvelope::decode` refuses every other length. | [ ] Partial: `sdk-libs/client/tests/merge.rs::the_merge_record_is_a_transfer_record_and_its_seed` round trips one envelope. No test pins the field order or refuses a 63-byte or 65-byte plaintext. |
| INV-KEYS-SEAL-13 | wire / H | `AuditEnvelope::encode` writes `tx_viewing_secret ‖ u8(R)`, and on an entry layout `u64be(default_note_bits)` then one 136-byte `owner_key ‖ nullifier_pk ‖ u64be(amount) ‖ mint ‖ input_commitment` per published input. | [x] `audit_stream.rs::the_solved_go_stream_reads_back_under_the_rust_layout` decodes each Go envelope, entry cases included, and encodes it to the same bytes. |
| INV-KEYS-SEAL-14 | pre / H | `AuditEnvelope::decode` refuses an `R` above the layout's published input count. | [x] `audit_stream.rs::envelopes_read_back_by_note_order`. |
| INV-KEYS-SEAL-15 | pre / H | `AuditEnvelope::decode` refuses a default note bit at or above `R`. | [x] `audit_stream.rs::envelopes_read_back_by_note_order`. |
| INV-KEYS-SEAL-16 | pre / H | `AuditEnvelope::decode` refuses a nonzero entry record whose default note bit is clear. | [x] `audit_stream.rs::envelopes_read_back_by_note_order`. |
| INV-KEYS-SEAL-17 | pre / H | `AuditEnvelope::decode` refuses a plaintext of any length other than the layout's envelope length. | [ ] Partial: `sdk-libs/client/src/crypto/disclosure/tests.rs::an_entry_record_of_another_length_is_refused` refuses a short and a widened entry envelope through the sealed open. No test refuses one trailing byte, and no test covers a layout without entry capacity. |
| INV-KEYS-SEAL-18 | pre / H | `decode_recipient_keys` returns exactly `m` keys and refuses every plaintext whose length is not `32 m`. | [x] `audit_stream.rs::recipient_keys_read_back_per_published_slot`. |
| INV-KEYS-SEAL-19 | wire / H | For every slot `i`, `recipient_key(i)` starts at counter `K0 + 2 i` and transmits 32 bytes when `i < m` and no byte otherwise. | [x] `audit_stream.rs::no_counter_serves_two_regions_and_every_region_holds_its_bytes`, `a_compact_frame_keeps_every_key_counter`, `the_ranges_are_the_shared_vector`, `sdk-libs/client/src/crypto/disclosure/tests.rs::a_recipient_key_opens_only_at_the_counter_of_its_slot`. |
| INV-KEYS-SEAL-20 | pre / H | `split_keys` refuses a sealed key region whose length differs from the layout's `keys_len`. | [x] `audit_stream.rs::the_auditor_message_holds_the_voucher_key_only_on_a_vouched_layout`. |
| INV-KEYS-SEAL-21 | pre / H | `AuditorMessage::parse` returns `None` unless the first byte is `0x04` and the exit record bytes are exactly `exits` times the published exit record length. | [x] `audit_stream.rs::the_auditor_message_cuts_by_the_layout_and_the_exits`. |
| INV-KEYS-SEAL-22 | pre / H | `RecipientPayload::decode` refuses a nonzero byte after the declared data. | [x] `audit_stream.rs::the_recipient_payload_reads_back_at_one_width`. |
| INV-KEYS-SEAL-23 | pre / H | `RecipientPayload::encode_padded(width)` refuses a width below `2 + data.len()` or above `u16::MAX` with `OutputBodyLength`. | [ ] Partial: `audit_stream.rs::the_recipient_payload_reads_back_at_one_width` refuses the short widths. A width above `u16::MAX` is not tested. |
| INV-KEYS-SEAL-24 | post / H | Every payload `RecipientPayload::encode_padded(width)` accepts has exactly `width` bytes. | [x] `audit_stream.rs::the_recipient_payload_reads_back_at_one_width`. |
| INV-KEYS-SEAL-25 | pre / H | `ring_body_output` returns `None` unless the data is `Encrypted(u32le(len) ‖ 10 ‖ u16le(body_len) ‖ body ‖ reply)` with `len` equal to the byte count after it and a body of at least 105 bytes. | [ ] Partial: `audit_stream.rs::a_ring_body_opens_with_its_record` reads one body and refuses a short body at encode. No test hands the reader a wrong scheme byte, a wrong outer length or a short body. |
| INV-KEYS-SEAL-26 | post / H | For every body length, `RecordForm::of_recipient_body` yields every record form whose record and payload fit that length. | [x] `sdk-libs/transaction/tests/rails.rs::a_reader_without_the_frame_opens_every_form_from_the_body` opens each of the four forms on both rails. |
| INV-KEYS-SEAL-27 | post / H | Under every label, a secret other than the sealer's gives a keystream other than the sealer's. | [ ] Missing: no test compares the keystreams of two secrets. `sdk-libs/client/tests/anonymous_sync.rs::a_bad_body_opens_nothing_and_a_foreign_slot_stays_a_miss_under_auditor_recovery` shows that a foreign body opens no note through the client. |

## List salt

Source: [list salt](../src/list_salt.rs) `ListSaltHolder::list_salt`, `derive`.

| ID | Kind / severity | Invariant | Coverage / required assertions |
|---|---|---|---|
| INV-KEYS-SALT-01 | wire / H | For every viewing secret and ring program id, `list_salt` equals HKDF-SHA256 under extract salt `TSPP/view_root/x25519/v1` and expand info `CRING/mrg_seed/v1 ‖ ring_program_id ‖ LIST_SALT_LABEL` into bytes 1 to 31, with byte 0 zero and the low bit of byte 31 set. | [x] `list_salt.rs::the_wallet_gives_the_pinned_salt_and_tag` against `test-vectors/registry/list_salt.json`. This crate writes the vector, and the TypeScript SDK reads it. |
| INV-KEYS-SALT-02 | post / H | Every list salt has byte 0 equal to zero and is not all zero. | [x] `list_salt.rs::a_salt_is_nonzero_and_canonical`. |
| INV-KEYS-SALT-03 | post / H | For one viewing secret, two different ring program ids give two different salts. | [x] `list_salt.rs::a_salt_opens_one_ring`. |
| INV-KEYS-SALT-04 | frame / H | A change of the nullifier secret leaves the list salt unchanged. | [x] `list_salt.rs::the_nullifier_secret_a_ring_auditor_holds_derives_no_salt`. |

## Ring address and payment request

Source: [ring address](../src/ring_address.rs) `RingAddress::{enrolled, check_list_salt, to_bytes,
from_bytes, from_base58}`, `checksum`, `RingPaymentRequest` `Display` and `FromStr`,
`replace_once`, `parse_amount`, `parse_mint`, `parse_tag`, `parse_label`.

| ID | Kind / severity | Invariant | Coverage / required assertions |
|---|---|---|---|
| INV-KEYS-ADDR-01 | wire / H | `RingAddress::to_bytes` writes `recipient ‖ ring_program_id ‖ list_salt ‖ checksum`, with `checksum` the first 4 bytes of `SHA256("zolana/ring-addr" ‖ body)`. | [x] `ring_address.rs::shared_vectors_match` against `test-vectors/wallet/ring_address.json`, which the TypeScript SDK reads. |
| INV-KEYS-ADDR-02 | pre / H | `RingAddress::from_bytes` refuses a payload whose checksum differs from the checksum of its body with `InvalidRingAddressChecksum`. | [x] `ring_address.rs::shared_vectors_match`, case `bad_checksum`. |
| INV-KEYS-ADDR-03 | pre / H | `RingAddress::from_bytes` refuses a payload of any length other than `RING_ADDRESS_PAYLOAD_LEN` with `InvalidRingAddressLength`. | [x] `ring_address.rs::shared_vectors_match`, cases `short_payload` and `long_payload`. |
| INV-KEYS-ADDR-04 | pre / H | `check_list_salt` accepts a salt only when the salt is zero exactly when the nullifier key is `ZERO_NULLIFIER_PUBKEY` and the salt is canonical. `from_bytes` and `enrolled` refuse every other pair with `InvalidRingAddressListSalt`. | [x] `ring_address.rs::shared_vectors_match`, cases `zero_salt_on_keyed_address` and `non_canonical_salt`. `ring_address.rs::a_zero_key_address_carries_salt_zero_and_nothing_else`. `list_salt.rs::an_enrolled_address_carries_the_wallet_salt_and_a_zero_secret_has_none`. |
| INV-KEYS-ADDR-05 | pre / H | `RingAddress::from_bytes` refuses a recipient whose keys do not decode, with the keypair error. | [x] `ring_address.rs::shared_vectors_match`, case `bad_viewing_key` gives `InvalidPublicKey`. |
| INV-KEYS-ADDR-06 | post / H | `RingAddress::enrolled(holder, ring)` names exactly `holder.address()` and `holder.list_salt(ring)`. | [ ] Partial: `list_salt.rs::an_enrolled_address_carries_the_wallet_salt_and_a_zero_secret_has_none` checks a `ShieldedKeypair`. A rotated `RotatingRingKeys` is not tested. |
| INV-KEYS-ADDR-07 | pre / H | `RingPaymentRequest::from_str` refuses a text that does not start with `zolana:` after trimming, with `InvalidRingAddressScheme`. | [x] `ring_address.rs::shared_vectors_match`, case `missing_scheme`. |
| INV-KEYS-ADDR-08 | pre / H | `from_str` refuses every query key other than `amount`, `mint`, `label` and `tag` with `UnknownRingPaymentKey`. | [x] `ring_address.rs::shared_vectors_match`, cases `deposit_tag_key` and `unknown_query_key`. |
| INV-KEYS-ADDR-09 | pre / H | `from_str` refuses a second occurrence of every query key with `DuplicateRingPaymentKey`. | [ ] Partial: `ring_address.rs::shared_vectors_match` repeats `amount` only. |
| INV-KEYS-ADDR-10 | pre / H | The `amount` value is accepted exactly when it is a nonempty string of ASCII digits whose value is at most `u64::MAX`. | [x] `src/ring_address.rs::tests::amount_rejects_signs_blanks_and_overflow`, `ring_address.rs::shared_vectors_match`, cases `fractional_amount` and `signed_amount`. |
| INV-KEYS-ADDR-11 | pre / H | The `tag` value is accepted exactly when it is base58 text that decodes to 32 bytes. | [ ] Partial: `ring_address.rs::shared_vectors_match` refuses `hex_tag` and a 31-byte `short_tag`. A text that decodes to 33 bytes is not tested. |
| INV-KEYS-ADDR-12 | pre / H | The `mint` value is accepted exactly when it parses as a base58 address. | [x] `ring_address.rs::shared_vectors_match`, case `bad_mint`. |
| INV-KEYS-ADDR-13 | pre / H | `from_str` refuses a `label` with a `%` not followed by two hex digits, or one that does not decode to UTF-8, with `InvalidRingPaymentLabel`. | [x] `src/ring_address.rs::tests::label_rejects_incomplete_or_non_utf8_escapes`, `ring_address.rs::shared_vectors_match`, case `bad_label_escape`. |
| INV-KEYS-ADDR-14 | wire / H | `Display` writes `zolana:` and the base58 address, then each present field in the order amount, mint, label, tag, the first after `?` and each later one after `&`. The label is percent encoded outside the RFC 3986 unreserved set, and the tag is base58. | [x] `ring_address.rs::shared_vectors_match`, `ring_address.rs::request_round_trips_every_field`, `src/ring_address.rs::tests::label_escapes_everything_outside_the_unreserved_set`. |

## Key rotation

Source: [key holder](../src/keys.rs) `RotatingRingKeys::{new, with_retired, rotate, holding,
identity_viewing_pubkey}`, `HeldKeys::held`, [list salt](../src/list_salt.rs)
`ListSaltHolder for RotatingRingKeys`.

| ID | Kind / severity | Invariant | Coverage / required assertions |
|---|---|---|---|
| INV-KEYS-ROTATE-01 | post / H | After `rotate(next)`, the address viewing key is exactly `next.pubkey()`, and `viewing_public_keys()` lists `next` first, then every earlier key in its earlier order. | [x] `src/keys.rs::tests::a_rotated_holder_answers_under_each_key_and_refuses_another`. |
| INV-KEYS-ROTATE-02 | frame / H | `rotate` leaves the address signing key unchanged. | [x] `sdk-libs/client/tests/anonymous_sync.rs::a_rotated_wallet_finds_notes_under_both_keys_through_a_restart_and_a_recovery`. |
| INV-KEYS-ROTATE-03 | frame / H | `rotate` leaves the address nullifier key unchanged. | [x] `sdk-libs/client/tests/anonymous_sync.rs::a_rotated_wallet_finds_notes_under_both_keys_through_a_restart_and_a_recovery`. `verify_spendable` under a holder built from the identity keypair accepts the note paid to the rotated address. The test makes no direct assertion on the key. |
| INV-KEYS-ROTATE-04 | frame / H | `rotate` and `with_retired` leave `identity_viewing_pubkey()` unchanged. | [ ] Partial: `cli/src/commands/wallet/identity/viewing_keys.rs::tests::the_holder_takes_the_newest_file_as_current_and_opens_a_note_to_the_identity_key` asserts it after two rotations. `with_retired` is not tested. |
| INV-KEYS-ROTATE-05 | frame / H | For every ring, `rotate` and `with_retired` leave `list_salt(ring)` unchanged. | [ ] Partial: `list_salt.rs::a_rotation_keeps_the_salt_a_final_tag_holds` covers `rotate`. `with_retired` is not tested. |
| INV-KEYS-ROTATE-06 | post / H | For every key passed to `with_retired`, `ring_delivery_secrets` answers a request under that key. | [x] `go_slot_vectors.rs::a_retired_key_answers_the_slot_secret_of_its_body` against the Go vector. |

## Redaction

Source: [redacted debug](../src/redacted_debug.rs) `redacted_debug!`, [records](../src/audit_records.rs)
`MergeRecord`, `MergeEnvelope`, `AuditEnvelope`, [ring address](../src/ring_address.rs)
`RingAddress`, `RingPaymentRequest`, [key holder](../src/keys.rs) `RotatingRingKeys`,
[slot secret](../src/slot_secret.rs) `SlotSecret`, `DeliveredSlot`,
[stream cipher](../src/stream_cipher.rs) `RingStream`.

| ID | Kind / severity | Invariant | Coverage / required assertions |
|---|---|---|---|
| INV-KEYS-REDACT-01 | frame / H | The `Debug` output of a `MergeRecord` does not contain its seed. | [x] `debug_redaction.rs::a_merge_record_prints_neither_its_seed_nor_its_blinding`. |
| INV-KEYS-REDACT-02 | frame / H | The `Debug` output of a `MergeEnvelope` does not contain its `eph_sk`. | [x] `debug_redaction.rs::a_merge_envelope_prints_no_ephemeral_secret`. |
| INV-KEYS-REDACT-03 | frame / H | The `Debug` output of an `AuditEnvelope` does not contain its `tx_viewing_secret`. | [x] `debug_redaction.rs::an_audit_envelope_prints_no_transaction_viewing_secret`. |
| INV-KEYS-REDACT-04 | frame / H | The `Debug` output of a `RingAddress` contains no recipient key and not the list salt. | [ ] Partial: `ring_address.rs::a_request_prints_none_of_its_recipient_keys` checks the viewing key and the signing key. `debug_redaction.rs::a_payment_request_prints_neither_its_list_salt_nor_its_tag` checks the list salt. The nullifier key is not checked. |
| INV-KEYS-REDACT-05 | frame / H | The `Debug` output of a `RingPaymentRequest` does not contain its tag. | [x] `debug_redaction.rs::a_payment_request_prints_neither_its_list_salt_nor_its_tag`. |
| INV-KEYS-REDACT-06 | frame / H | `RotatingRingKeys`, `SlotSecret`, `DeliveredSlot` and `RingStream` implement no `Debug`. | [ ] Missing: no test checks the trait set. |

## Open questions, not coverage claims

- SPEC_DIVERGENCE: `src/audit_records.rs:298-307` (`MergeRecord::decode`) accepts a record of
  kind 0 or 2 vs [Disclosure, Merges](../../../docs/spec/crypto/disclosure.md#merges), which
  fixes the merge record at kind 1. The client rebuilds the merge commitment after the decode.
- INSUFFICIENT_INFO: the code and the docs name no rule on printing a note opening.
  `OutputRecord`, `EntryRecord`, and `MergeRecord` through its `record` field print the amount,
  owner key and mint in `Debug`. `OutputRecord` hides its blinding
  (`debug_redaction.rs::an_output_record_prints_no_blinding`).
- INSUFFICIENT_INFO: [Hashing, slot secret](../../../docs/spec/crypto/hashing.md#slot-secret)
  writes `clamp(Poseidon(CRDK, sk_0, sk_1, i))` and names no byte order for the clamp.
  `dummy_recipient_key` reads the big-endian Poseidon bytes as the little-endian scalar.
  INV-KEYS-SLOT-07 rests on the code.
- No ring line carries an authentication tag. The decoders here refuse lengths and bit
  patterns only, so a wrong key can yield a record that decodes. The reader's commitment
  rebuild decides acceptance, and the client checklist holds that rule.
- `RotatingRingKeys::new(keypair)` takes `keypair.viewing_key` as the identity key and the list
  root. A holder rebuilt as `new(&keypair_with_current_key).with_retired(..)` derives its list
  salt and identity key from the current key, not from the enrolled one. The CLI builds its
  holder from the identity and rotates. `sdk-libs/client/tests/anonymous_sync.rs` and
  `program-tests/custom-ring/tests/ring/sanity.rs` build holders the other way.
- The `records` cases of `test-vectors/audit/auditor-stream.json` seal records under
  `CRING/adt` at counter `R0 + i R`. The code seals each record under `CRING/rcp` and its slot
  secret from counter 2, as [Disclosure, Records](../../../docs/spec/crypto/disclosure.md#records)
  states. The cases pin the stream schedule only, and no record reader reads them.
