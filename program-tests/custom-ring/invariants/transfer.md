# Transfer Invariants

Covers `Transfer` (tag 55), `AuthorityTransfer` (tag 56) and `ApproveTransfer` (tag 65).

`Transfer` and `AuthorityTransfer` run one processor, `process_transfer_ix`, over one frame parse, one account check and one statement. The `INV-TRANSFER-*` entries hold for both tags unless the statement names a rail. The `INV-AUTH-TRANSFER-*` entries hold for the authority rails alone. Invariants shared with `Merge` (registry slots, the `ring_auth` slot, the forwarded payer, the SPP program check, the ring proof, live roots) live in `cross-cutting.md`.

The processor runs these stages in order, and every refusal below names the first stage that fails: load the ring account, fix the rail, parse the frame, check the accounts, expand the SPP data, verify the ring proof, call SPP.

## Transfer

### Rail Selection

- [x] **INV-TRANSFER-01: the tag and the ring mode fix the rail**
  - Covered by: `sdk-libs/client/tests/transact.rs` `the_rail_follows_the_instruction_and_the_ring_mode` (library), `program-tests/custom-ring/tests/rules/transact_rules.rs` `an_anonymous_ring_delegate_settles_on_the_authority_rail` (LiteSVM)
  - Kind: precondition
  - Statement: for every call, the rail is exactly `Confidential` for tag 55 on a ring of mode 0, `Anonymous` for tag 55 on mode 2, `Delegate` for tag 56 on mode 0 and `AnonymousDelegate` for tag 56 on mode 2. No frame byte changes the rail.
  - Location: `programs/custom-ring/src/instructions/transfer/processor.rs:33-37` (`fn process_transfer_ix`), `program-libs/interface/src/instructions/transact.rs:121-142` (`fn RingRail::of`, `fn transfer`, `fn delegate`)
  - Severity: Critical (a frame that picked the delegate rail would skip owner signatures)
  - Suggested test: positive per (tag, mode) pair, harness: library and LiteSVM

- [x] **INV-TRANSFER-02: a frame of the other mode never reaches the ring proof**
  - Covered by: `sdk-libs/client/tests/core_opt_validation.rs` `each_authorization_encodes_its_flags_and_parses_back_under_its_rail` (the exact refusal of each confidential frame under the anonymous rail and of each anonymous frame under the confidential rail) (library), `program-tests/custom-ring/tests/rules/anonymous_rules.rs` `an_anonymous_policy_ring_refuses_a_confidential_transfer` (`is_err` only) (LiteSVM)
  - Kind: precondition
  - Statement: every confidential frame sent to an anonymous ring, and every anonymous frame sent to a confidential ring, returns Err before the ring proof check.
  - Location: `program-libs/interface/src/instructions/transact.rs:653-832` (`fn RingTransactData::parse`, the rail's flag checks and the `match rail` at 762-783)
  - Error: the first length or flag refusal the other layout meets, `RingTransactLength = 8200` or `FlagNotAllowed = 8202`
  - Severity: High
  - Suggested test: negative with the exact code, harness: LiteSVM

### Instruction Data Validation

- [x] **INV-TRANSFER-03: the frame length follows the flags and the shape exactly**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `trailing_or_missing_bytes_disagree_with_the_shape`, `a_truncated_or_appended_ciphertext_is_refused` (LiteSVM), `sdk-libs/client/tests/transact.rs` `the_counts_fix_every_length_and_trailing_bytes_are_refused` (library)
  - Kind: precondition
  - Statement: every frame with one byte more or one byte less than the flags, the header counts and the stream layout fix returns Err before every account check.
  - Location: `program-libs/interface/src/instructions/transact.rs:653-832` (`fn RingTransactData::parse`, `bytes.rest()` check at 796-798), `transact.rs:846-861` (`fn validate_under`, host-built data)
  - Error: `CustomRingError::RingTransactLength = 8200`
  - Severity: Medium
  - Suggested test: fuzz over truncation and extension, harness: library

- [x] **INV-TRANSFER-04: a flag the rail forbids is refused**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `flags_follow_the_rings_mode_and_policy`, `reply_metadata_reaches_the_key_on_the_anonymous_rail_and_no_other` (LiteSVM), `sdk-libs/client/tests/transact.rs` `flags_the_rail_forbids_are_refused`, `anonymous_header_flags_off_the_anonymous_rail_are_refused`, `the_retired_extension_bits_are_refused`, `the_in_proof_owner_is_refused_off_the_confidential_rail`, `the_delegate_reads_no_policy_and_spends_no_default_note` (library), `sdk-libs/client/tests/core_opt_validation.rs` `each_authorization_encodes_its_flags_and_parses_back_under_its_rail` (`OWNER_IN_PROOF`, `PUBLIC_OWNERS` and `DEFAULT_OWNER_TAG` under each other rail) (library)
  - Kind: precondition
  - Statement: every frame returns Err when one of these holds: `OWNER_IN_PROOF` off the confidential rail, `REPLY_METADATA` off an anonymous frame, `POLICY` or `OWN_ROOT` on a delegate rail, `CURATOR` without `POLICY`, `POLICY` without `OWN_ROOT`, an unknown extension bit, a cached selector on a delegate rail, `PUBLIC_OWNERS` or `DEFAULT_OWNER_TAG` off the anonymous rail, `PUBLIC_OWNERS` with `DEFAULT_OWNER_TAG`, `ENTRY` on a delegate rail.
  - Location: `program-libs/interface/src/instructions/transact.rs:1221-1236` (`fn TransactFlags::check`), `transact.rs:1434-1445` (`fn FrameFlags::check`), `transact.rs:1116-1119` (`fn FrameExtensions::read`), `transact.rs:1267-1275` (`fn check_extensions`)
  - Error: `CustomRingError::FlagNotAllowed = 8202`
  - Severity: High
  - Suggested test: negative per flag and rail, harness: library

- [x] **INV-TRANSFER-05: the proof sections match the rail**
  - Covered by: `sdk-libs/client/tests/core_opt_validation.rs` `each_authorization_encodes_its_flags_and_parses_back_under_its_rail` (each of the six authorizations parses back as itself under its rail and returns `FlagNotAllowed` or `RingTransactLength` under the other three), `checked_frames_accept_the_supported_authorization_modes` (library)
  - Kind: precondition
  - Statement: the parse reads the hidden owner's commitment section exactly on the anonymous rail without `PUBLIC_OWNERS`, and the in-proof owner section exactly on the confidential rail under `OWNER_IN_PROOF`. Every frame whose bytes or flags put a section under another rail returns Err. `RailAuthorization` holds each section only in the variant of its rail, so no parsed value carries a section of another rail.
  - Location: `program-libs/interface/src/instructions/transact.rs:762-783` (`fn RingTransactData::parse`, the `match rail` that builds `RailAuthorization`), `transact.rs:283-292` (`enum RailAuthorization`)
  - Error: `CustomRingError::FlagNotAllowed = 8202` for a flag the rail forbids / `CustomRingError::RingTransactLength = 8200` for section bytes the rail does not read
  - Severity: High
  - Suggested test: negative per rail, harness: library

- [ ] **INV-TRANSFER-06: the approval byte is a valid mask**
  - Partial coverage: `sdk-libs/client/tests/transact.rs` `the_approval_byte_sits_past_the_header` asserts `is_err` for bits 2 and 5, not the exact code.
  - Kind: precondition
  - Statement: every frame whose byte 4 is not a valid `ApprovalMask` returns Err.
  - Location: `program-libs/interface/src/instructions/transact.rs:662` (`fn RingTransactData::parse`)
  - Error: `CustomRingError::FlagNotAllowed = 8202`
  - Severity: Medium
  - Suggested test: negative over all 256 values with the exact code, harness: library

- [x] **INV-TRANSFER-07: the shape index names a keyed SPP shape of the rail**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `a_delegate_shape_without_a_ring_authority_key_is_refused`, `a_default_ring_spend_takes_the_entry_key_up_to_eight_inputs` (LiteSVM), `sdk-libs/client/tests/transact.rs` `counts_outside_the_keyed_shapes_are_refused` (library)
  - Kind: precondition
  - Statement: every frame returns Err when its shape index is outside `SPP_SUPPORTED_SHAPES`, when the rail's SPP selector does not support the shape, or when no catalog key holds its rail, nullifier count, output capacity, entry flag and wide flag.
  - Location: `program-libs/interface/src/instructions/transact.rs:1291-1295` (`fn FrameHeader::read`), `transact.rs:190-212` (`fn RingRail::key_shape`), `program-libs/interface/src/proof/key_catalog.rs:77-92` (`fn FrameKey::shape`)
  - Error: `CustomRingError::RingShapeUnkeyed = 8201`
  - Severity: High
  - Suggested test: negative per unkeyed shape, harness: library

- [ ] **INV-TRANSFER-08: the published input count lies in 1 to the SPP input width**
  - Partial coverage: `sdk-libs/client/tests/transact.rs` `counts_outside_the_keyed_shapes_are_refused` covers a count above the width. No test sends a zero count.
  - Kind: precondition
  - Statement: every frame whose nullifier count is 0 or strictly greater than the shape's `n_in` returns Err.
  - Location: `program-libs/interface/src/instructions/transact.rs:208-210` (`fn RingRail::key_shape`), `transact.rs:867-869` (`fn validate_under`, host-built data)
  - Error: `CustomRingError::RingTransactLength = 8200`
  - Severity: Medium
  - Suggested test: negative at 0 and at `n_in + 1`, harness: library

- [x] **INV-TRANSFER-09: the published slot count lies in 1 to the shape's output width**
  - Covered by: `sdk-libs/client/tests/transact.rs` `the_output_count_stays_between_one_and_the_capacity` (library)
  - Kind: precondition
  - Statement: every frame whose slot count `m` is 0 or strictly greater than the shape's `n_out` returns Err.
  - Location: `program-libs/interface/src/instructions/transact.rs:1331-1337` (`fn FrameHeader::check_output_count`), called at `transact.rs:1313` (`fn FrameHeader::read`)
  - Error: `CustomRingError::OutputCountOutOfRange = 8257`
  - Severity: Medium
  - Suggested test: negative at 0 and at `n_out + 1`, harness: library

- [x] **INV-TRANSFER-10: no bitmap bit names a slot or an input past its count**
  - Covered by: `sdk-libs/client/tests/transact.rs` `the_exit_bitmap_stays_inside_the_count`, `bitmap_bits_past_the_count_are_refused` (library)
  - Kind: precondition
  - Statement: every frame whose exit bitmap sets a bit at or above `m`, or whose second-tree bitmap sets a bit at or above the nullifier count, returns Err.
  - Location: `program-libs/interface/src/instructions/transact.rs:700, 787` (`fn RingTransactData::parse`), `transact.rs:870-878` (`fn validate_under`, host-built data), `program-libs/interface/src/codec/bytes.rs:68-83` (`fn Bytes::bitmap`)
  - Error: `CustomRingError::InvalidInstructionData = 8100`
  - Severity: High (a stray exit bit changes the record chain the statement binds)
  - Suggested test: negative per bitmap, harness: library

- [x] **INV-TRANSFER-11: every record sits at its slot length at the head of its body**
  - Covered by: `sdk-libs/client/tests/transact.rs` `a_record_off_its_slot_length_is_refused`, `every_non_exit_slot_forwards_its_record`, `a_body_past_its_plain_length_needs_has_data` (host-built data) (library), `program-libs/interface/tests/transact_frames.rs` `every_single_byte_change_and_truncation_parses_as_parse_and_validate` (no mutated vector frame that `parse` accepts fails `validate_under`) (library)
  - Kind: precondition
  - Statement: for every slot, the record length is exactly the length its kind takes under the frame's record form, a recipient body starts with the record, and without `HAS_DATA` every body has its plain length. Every other frame returns Err. The parse takes each record at its kind's length at the head of its body and each body at its plain length without `HAS_DATA`, so no parsed frame breaks this, and `validate_under` refuses host-built data that does.
  - Location: `program-libs/interface/src/instructions/transact.rs:712-744` (`fn RingTransactData::parse`), `transact.rs:505-511` (`fn RingOutput::record_leads_body`), `transact.rs:846-861` (`fn validate_under`, host-built data)
  - Error: `CustomRingError::RingTransactLength = 8200`
  - Severity: High (the record chain hashes the record bytes SPP receives)
  - Suggested test: negative per slot kind, harness: library

- [x] **INV-TRANSFER-12: a non-exit tag keeps its canonical leading zeros**
  - Covered by: `sdk-libs/client/tests/transact.rs` `an_exit_keeps_its_whole_tag_and_a_ring_slot_refuses_a_longer_one` (the refusal on host-built data) (library), `program-libs/interface/tests/transact_frames.rs` `every_single_byte_change_and_truncation_parses_as_parse_and_validate` (no mutated vector frame that `parse` accepts fails `validate_under`) (library)
  - Kind: precondition
  - Statement: every frame with a non-exit slot whose owner tag differs from `slot_tag(owner_tag)` returns Err. The parse restores every non-exit tag through `unframed_slot_tag`, so no parsed frame breaks this, and `validate_under` refuses host-built data that does.
  - Location: `program-libs/interface/src/instructions/transact.rs:722-726` (`fn RingTransactData::parse`), `transact.rs:857` (`fn validate_under`, host-built data)
  - Error: `CustomRingError::RingTransactLength = 8200`
  - Severity: Medium
  - Suggested test: negative, harness: library

- [x] **INV-TRANSFER-13: bodies and messages fit their length prefixes**
  - Covered by: `sdk-libs/client/tests/transact.rs` `a_data_frame_carries_each_length_and_one_payload_width` (library)
  - Kind: precondition
  - Statement: every frame returns Err when a slot carries more than `u16::MAX` bytes past its record, when an application message is longer than `u16::MAX` bytes, or when two recipient bodies under `HAS_DATA` differ in length. The parse reads each of these lengths from a `u16` prefix, so on chain only the width check refuses, and `validate_under` refuses host-built data past `u16::MAX`.
  - Location: `program-libs/interface/src/instructions/transact.rs:829` (`fn RingTransactData::parse`), `transact.rs:881-895` (`fn validate_under`, host-built data), `transact.rs:900-914` (`fn check_payload_width`)
  - Error: `CustomRingError::OutputBodyLength = 8253`
  - Severity: Medium
  - Suggested test: negative at `u16::MAX + 1` and at two widths, harness: library

- [x] **INV-TRANSFER-14: replies are one length per frame, one per recipient body**
  - Covered by: `sdk-libs/client/tests/transact.rs` `reply_metadata_is_one_length_on_the_anonymous_frame_alone` (library), `sdk-libs/client/tests/core_opt_validation.rs` `reply_construction_rejects_zero_and_short_chunks`, `reply_construction_rejects_incomplete_chunks`, `checked_frames_require_one_reply_per_recipient_body` (library)
  - Kind: precondition
  - Statement: every frame under `REPLY_METADATA` returns Err when its reply length is below `MIN_REPLY_LEN` or the reply region is not exactly the length times the number of recipient bodies. The parse takes the region at exactly the length times the body count, so on chain only the length floor refuses.
  - Location: `program-libs/interface/src/instructions/transact.rs:524-546` (`fn Replies::new`, `fn checked`), `transact.rs:746-755` (`fn RingTransactData::parse`)
  - Error: `CustomRingError::ReplyLength = 8259`
  - Severity: Medium
  - Suggested test: negative, harness: library

- [x] **INV-TRANSFER-15: at most 254 application messages ride beside the auditor message**
  - Covered by: `sdk-libs/client/tests/frame_bounds.rs` `the_message_limit_is_spp_s_beside_the_auditor_message` (library)
  - Kind: precondition
  - Statement: every frame with strictly more than `MAX_APPLICATION_MESSAGES` (254) application messages returns Err before the message list is allocated.
  - Location: `program-libs/interface/src/instructions/transact.rs:1127-1143` (`fn FrameExtensions::read`), `transact.rs:1250-1256` (`fn check_message_count`)
  - Error: `CustomRingError::TooManyApplicationMessages = 8269`
  - Severity: Medium
  - Suggested test: negative at 255, harness: library

- [x] **INV-TRANSFER-16: public legs keep SPP's count and amount rules under ring errors**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `public_legs_are_refused_with_the_rings_own_errors`, `public_legs_fit_the_account_list_and_the_leg_cap` (LiteSVM), `sdk-libs/client/tests/frame_bounds.rs` `public_leg_refusals_carry_ring_errors` (library)
  - Kind: precondition
  - Statement: every frame with strictly more than `MAX_INTERFACE_TRANSFERS` legs returns `TooManyPublicLegs`, and every frame with a leg of amount 0 returns `ZeroPublicLegAmount`.
  - Location: `program-libs/interface/src/instructions/transact.rs:1258-1264` (`fn check_public_legs`), called at `transact.rs:830` (`fn RingTransactData::parse`)
  - Error: `CustomRingError::TooManyPublicLegs = 8243` / `CustomRingError::ZeroPublicLegAmount = 8225`
  - Severity: Medium
  - Suggested test: negative at 256 legs and at a zero leg, harness: LiteSVM

- [x] **INV-TRANSFER-17: the SPL withdrawal owner list matches the legs**
  - Covered by: `sdk-libs/client/tests/transact.rs` `withdrawal_owner_metadata_is_required_in_transfer_order` (host-built data) (library), `program-libs/interface/tests/transact_frames.rs` `every_single_byte_change_and_truncation_parses_as_parse_and_validate` (no mutated vector frame that `parse` accepts fails `validate_under`) (library)
  - Kind: precondition
  - Statement: every frame returns Err unless it names an own revision exactly under `OWN_ROOT` and the SPL withdrawal owner list holds exactly one entry per SPL withdrawal leg under `OWN_ROOT` and none otherwise. The parse reads both sections by the flag and the leg count, so a frame that breaks this fails as a short read or with trailing bytes. `flags` derives `OWN_ROOT` from `NamedRoots`, so no host-built value names an own revision without the flag.
  - Location: `program-libs/interface/src/instructions/transact.rs:666-681, 694-698` (`fn RingTransactData::parse`), `transact.rs:846-853` (`fn validate_under`, host-built data)
  - Error: `CustomRingError::RingTransactLength = 8200`
  - Severity: Medium
  - Suggested test: negative per count, harness: library

- [x] **INV-TRANSFER-18: an SPP proof point off the curve is refused before either pairing**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `an_off_curve_spp_point_is_refused_before_either_pairing` (LiteSVM), `sdk-libs/client/tests/transact.rs` `an_spp_point_off_the_curve_is_refused_at_expansion` (library)
  - Kind: precondition
  - Statement: every frame whose compressed SPP `proof_b` does not decompress returns Err after the account checks and before the ring proof check.
  - Location: `programs/custom-ring/src/instructions/transfer/processor.rs:57` (`fn process_transfer_ix`), `program-libs/interface/src/instructions/transact/expansion.rs:44-52` (`fn spp_proof_points`)
  - Error: `CustomRingError::InvalidSppProofPoint = 8218`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM

### Roots

- [x] **INV-TRANSFER-19: the rail and the ring fix the registries a transfer reads**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `a_bare_ring_reads_the_tracking_root_alone`, `a_ring_with_records_binds_its_own_root_without_a_policy`, `a_delegate_reads_no_policy_root_and_keeps_the_tracking_root` (LiteSVM)
  - Kind: state
  - Statement: the root read is exactly `None` on a delegate rail, `Policy` on a non-delegate rail of a ring with a written policy hash, `Own` on a non-delegate rail of a ring with records and no policy, and `None` otherwise. Every transfer also reads the tracking registry.
  - Location: `program-libs/interface/src/accounts/state.rs:427-433` (`fn RootRead::transfer`), `state.rs:327-332` (`fn root_registries`), `programs/custom-ring/src/instructions/transfer/accounts.rs:130` (`fn TransactCheck::validate`)
  - Severity: Critical (a skipped policy read would settle a transfer no rule screened)
  - Suggested test: positive per ring setup and rail, harness: LiteSVM

- [x] **INV-TRANSFER-20: the policy section is present exactly when the rail reads the policy root**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `flags_follow_the_rings_mode_and_policy`, `a_ring_without_a_policy_hash_reads_as_audit_only` (LiteSVM)
  - Kind: precondition
  - Statement: every frame with a policy section on a read other than `Policy` returns `UnexpectedPolicyProof`, and every frame without one on a `Policy` read returns `MissingPolicyProof`.
  - Location: `program-libs/interface/src/instructions/transact.rs:924-930` (`fn RingTransactData::roots`)
  - Error: `CustomRingError::UnexpectedPolicyProof = 8230` / `CustomRingError::MissingPolicyProof = 8229`
  - Severity: Critical
  - Suggested test: negative both ways, harness: LiteSVM

- [x] **INV-TRANSFER-21: the curator revision is present exactly when the ring names a curator**
  - Covered by: `program-libs/interface/tests/transact_frames.rs` `a_curator_revision_names_the_ring_s_curator_exactly` (both ways) (library)
  - Kind: precondition
  - Statement: every frame on a `Policy` read returns Err when it carries a curator revision and the ring's sources name no curator, or carries none and they name one.
  - Location: `program-libs/interface/src/instructions/transact.rs:931-935` (`fn RingTransactData::roots`)
  - Error: `CustomRingError::FlagNotAllowed = 8202`
  - Severity: Critical (an omitted curator root skips the curator's lists)
  - Suggested test: negative both ways, harness: LiteSVM

- [x] **INV-TRANSFER-22: the own revision is present exactly when the rail reads a policy root**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `a_bare_ring_reads_the_tracking_root_alone`, `a_ring_with_records_binds_its_own_root_without_a_policy` (LiteSVM)
  - Kind: precondition
  - Statement: every frame returns Err when it names an own revision and the read is `None`, or names none and the read is `Own` or `Policy`.
  - Location: `program-libs/interface/src/instructions/transact.rs:936-938` (`fn RingTransactData::roots`)
  - Error: `CustomRingError::FlagNotAllowed = 8202`
  - Severity: Critical (a ring with records holds the thresholds the approval mask reads)
  - Suggested test: negative both ways, harness: LiteSVM

- [x] **INV-TRANSFER-23: every bound root comes from its pinned registry at the frame's revision**
  - Covered by: `program-tests/custom-ring/tests/rules/storage_root_rules.rs` `a_policy_transfer_proves_under_a_live_storage_root`, `a_curator_revocation_refuses_the_next_subscriber_transfer` (LiteSVM), `program-tests/custom-ring/tests/rules/transact_rules.rs` `a_ring_with_records_binds_its_own_root_without_a_policy`, `a_delegate_reads_no_policy_root_and_keeps_the_tracking_root` (LiteSVM)
  - Kind: postcondition
  - Statement: the statement's `tracking_root`, `own_root` and `curator_root` are exactly the roots the pinned tracking, records and curator registries serve at the frame's revisions and the current slot, zero where the rail reads none, and `policy_hash` is exactly the ring account's stored hash on a `Policy` read and zero otherwise. A revision the registry does not serve returns Err.
  - Location: `program-libs/interface/src/instructions/transact.rs:944-957` (`fn RingTransactData::roots`), `program-libs/interface/src/accounts/state.rs:454-486` (`fn LiveRoots::own`, `fn curator`, `fn tracking`, `fn served`)
  - Error: `CustomRingError::RootNotServed = 8481`
  - Severity: Critical (a frame-chosen root would prove a revoked list state)
  - Suggested test: negative per registry with a retired root, harness: LiteSVM

- [x] **INV-TRANSFER-24: a delegate frame carries no approval bit**
  - Covered by: `program-libs/interface/tests/transact_frames.rs` `a_delegate_frame_carries_no_approval_bit` (library)
  - Kind: precondition
  - Statement: every frame on a delegate rail whose approval mask is not empty returns Err.
  - Location: `program-libs/interface/src/instructions/transact.rs:940-942` (`fn RingTransactData::roots`)
  - Error: `CustomRingError::UnexpectedApproval = 8241`
  - Severity: High (the delegate proof clears the mask, a set bit has no proof behind it)
  - Suggested test: negative per mask bit, harness: LiteSVM

### Co-signers

- [x] **INV-TRANSFER-25: the operation and the proven bits set the co-sign demand**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `optional_classes_follow_the_economic_operation`, `a_configured_withdrawals_record_demands_its_co_signer`, `a_proven_bit_fails_closed_without_an_approver_of_its_class`, `a_ring_with_no_record_settles_below_every_threshold_with_no_co_signer` (LiteSVM), `program-libs/interface/src/instructions/cosign.rs` `an_optional_class_without_a_record_demands_nothing`, `a_pure_deposit_needs_no_transfer_co_signer`, `a_proven_bit_needs_its_approver_with_or_without_a_record` (unit)
  - Kind: state
  - Statement: on every non-delegate rail the demanded classes are exactly `TRANSFERS` when `no_spend` is clear, `DEPOSITS` for a deposit leg, `WITHDRAWALS` for a withdrawal leg, and the approval class of each set mask bit, intersected with the optional classes the ring's counts cover plus both approval classes.
  - Location: `program-libs/interface/src/instructions/transact.rs:983-999` (`fn co_sign_requirement`), `program-libs/interface/src/instructions/cosign.rs:39-55` (`fn CoSignOperation::demand`, `fn CoSignScope::requirement`)
  - Severity: Critical (an undemanded class settles without its co-signer)
  - Suggested test: property over legs, mask and counts, harness: unit and LiteSVM

- [x] **INV-TRANSFER-26: a demanded pair names a co-signer record**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `a_demanded_co_signer_signs_with_the_configured_key` (record of another signer), `a_proven_bit_fails_closed_without_an_approver_of_its_class`, `a_configured_withdrawals_record_demands_its_co_signer` (LiteSVM)
  - Kind: precondition
  - Statement: for every demanded pair, the first account is a program-owned `CoSigner` account of exact size and discriminator 14. Every other account at that slot returns Err.
  - Location: `programs/custom-ring/src/instructions/accounts/loader.rs:354-355` (`fn CoSignerSlot::verify`), `loader.rs:38-47` (`fn load`), `loader.rs:70-80` (`fn OwnedAccount::load`)
  - Error: `CustomRingError::InvalidCoSignerRecord = 8260`
  - Severity: Critical
  - Suggested test: negative, harness: LiteSVM

- [x] **INV-TRANSFER-27: every pair covers a class still uncovered**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `a_co_signer_whose_scope_misses_the_demanded_class_is_refused`, `separate_approvers_settle_a_transaction_past_both_thresholds` (repeated record), `optional_classes_follow_the_economic_operation` (LiteSVM)
  - Kind: precondition
  - Statement: every pair whose record scope shares no class with the classes the earlier pairs left uncovered returns Err.
  - Location: `program-libs/interface/src/instructions/cosign.rs:89-99` (`fn CoSignRequirement::cover`), `cosign.rs:73-83` (`fn pairs`)
  - Error: `CustomRingError::CoSignerOutOfScope = 8262`
  - Severity: High
  - Suggested test: negative, harness: LiteSVM

- [x] **INV-TRANSFER-28: the pairs end exactly where the demand is covered**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `separate_approvers_settle_a_transaction_past_both_thresholds` (an extra pair shifts SPP's list and meets `MissingRingAuth`) (LiteSVM)
  - Kind: state
  - Statement: the pair window holds exactly as many pairs as the demand needs, and the account after the last pair starts the delegate slot or SPP's list. An extra pair moves `ring_auth` off its slot and returns Err.
  - Location: `programs/custom-ring/src/instructions/accounts/loader.rs:384-409` (`fn CoSigners::verify`), `program-libs/interface/src/instructions/transfer_layout.rs:71-95` (`fn TransferLayout::new`)
  - Error: `CustomRingError::MissingRingAuth = 8104`
  - Severity: High
  - Suggested test: negative with an extra pair, harness: LiteSVM

- [x] **INV-TRANSFER-29: a direct co-signer signs under its record's key**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `a_demanded_co_signer_signs_with_the_configured_key`, `the_program_reads_each_signer_where_the_layout_names_it` (LiteSVM)
  - Kind: precondition
  - Statement: for every pair whose second account is not the instructions sysvar, that account must sign (`MissingCoSigner`) and its address must equal the record's `signer` (`UnauthorizedCoSigner`).
  - Location: `programs/custom-ring/src/instructions/accounts/loader.rs:365-370` (`fn CoSignerSlot::verify`)
  - Error: `CustomRingError::MissingCoSigner = 8143` / `CustomRingError::UnauthorizedCoSigner = 8144`
  - Severity: Critical (co-signer impersonation)
  - Suggested test: negative both legs, harness: LiteSVM

- [x] **INV-TRANSFER-30: a sysvar pair needs a listed approval**
  - Covered by: `program-tests/custom-ring/tests/rules/program_owner_rules.rs` `a_forwarded_transfer_without_an_approval_of_its_call_is_refused` (LiteSVM)
  - Kind: precondition
  - Statement: for every pair that names the instructions sysvar, the transfer returns Err unless some instruction of the transaction is this program's `ApproveTransfer` naming the same ring and record.
  - Location: `programs/custom-ring/src/instructions/accounts/loader.rs:296-307` (`fn TransferApprovalSearch::find`), `loader.rs:310-328` (`fn read`)
  - Error: `CustomRingError::MissingTransferApproval = 8270`
  - Severity: Critical
  - Suggested test: negative, harness: LiteSVM

- [x] **INV-TRANSFER-31: a listed approval binds the exact call**
  - Covered by: `program-tests/custom-ring/tests/rules/program_owner_rules.rs` `a_forwarded_transfer_without_an_approval_of_its_call_is_refused` (other data, reordered accounts, other program id) (LiteSVM), `program-libs/interface/src/instructions/transfer_approval.rs` `the_digest_binds_the_program_the_data_and_the_account_order` (unit)
  - Kind: precondition
  - Statement: an approval counts only when its digest equals `sha256(program_id || u32 LE data length || data from the tag || every account address in order)` of the running transfer. Every other digest returns Err.
  - Location: `programs/custom-ring/src/instructions/accounts/loader.rs:270-276` (`fn TransferCall::digest`), `loader.rs:336` (`fn TransferApprovalSearch::read`), `program-libs/interface/src/instructions/transfer_approval.rs:40-56` (`fn TransferApproval::digest`)
  - Error: `CustomRingError::TransferApprovalOtherCall = 8271`
  - Severity: Critical (a forwarded signature covers every call its program makes)
  - Suggested test: negative per digest input, harness: LiteSVM

- [x] **INV-TRANSFER-32: a listed approval names the running top-level instruction**
  - Covered by: `program-tests/custom-ring/tests/rules/program_owner_rules.rs` `a_forwarded_transfer_without_an_approval_of_its_call_is_refused` (position 1) (LiteSVM)
  - Kind: precondition
  - Statement: an approval counts only when its position equals the index of the top-level instruction the transfer runs inside. Every other position returns Err.
  - Location: `programs/custom-ring/src/instructions/accounts/loader.rs:297, 334-335` (`fn TransferApprovalSearch::find`, `fn read`)
  - Error: `CustomRingError::TransferApprovalOtherPosition = 8272`
  - Severity: High
  - Suggested test: negative, harness: LiteSVM

- [x] **INV-TRANSFER-33: a listed approval carries its record signer's signature**
  - Covered by: `program-tests/custom-ring/tests/rules/program_owner_rules.rs` `an_approval_counts_only_under_the_signature_of_the_records_signer`, `an_account_other_than_the_instructions_sysvar_must_sign` (LiteSVM)
  - Kind: precondition
  - Statement: an approval counts only when its co-signer account is the record's `signer` (`UnauthorizedCoSigner`) and carries the transaction's signer flag (`MissingCoSigner`).
  - Location: `programs/custom-ring/src/instructions/accounts/loader.rs:329-333` (`fn TransferApprovalSearch::read`)
  - Error: `CustomRingError::UnauthorizedCoSigner = 8144` / `CustomRingError::MissingCoSigner = 8143`
  - Severity: Critical
  - Suggested test: negative both legs, harness: LiteSVM

- [x] **INV-TRANSFER-34: no co-signer is an owner of the run**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `a_withdrawal_co_signed_by_its_own_owner_is_refused`, `an_approval_co_signed_by_its_own_owner_is_refused` (LiteSVM)
  - Kind: precondition
  - Statement: on the confidential and anonymous rails, every transfer whose owner run holds an address of a verified co-signer returns Err.
  - Location: `programs/custom-ring/src/instructions/transfer/accounts.rs:262-267` (`fn check_owner_run`)
  - Error: `CustomRingError::CoSignerIsOwner = 8231`
  - Severity: Critical (dual control would collapse to one party)
  - Suggested test: negative, harness: LiteSVM

- [x] **INV-TRANSFER-35: no co-signer is the in-proof owner**
  - Covered by: `program-tests/custom-ring/tests/rules/in_proof_owner_rules.rs` `an_in_proof_owner_co_signing_its_own_transfer_is_refused` (LiteSVM)
  - Kind: precondition
  - Statement: on the confidential rail, every frame whose in-proof owner key equals the address of a verified co-signer returns Err.
  - Location: `programs/custom-ring/src/instructions/transfer/accounts.rs:202-206` (`fn TransactCheck::validate`)
  - Error: `CustomRingError::CoSignerIsOwner = 8231`
  - Severity: Critical
  - Suggested test: negative, harness: LiteSVM

### Owners and Funders

- [x] **INV-TRANSFER-36: every account of the owner run signs**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `every_confidential_owner_slot_signs`, `an_anonymous_owner_run_signs_and_another_signer_reaches_the_proof`, `the_program_reads_each_signer_where_the_layout_names_it` (LiteSVM), `program-tests/custom-ring/tests/rules/program_owner_rules.rs` `a_pda_owner_reaches_the_ring_proof_only_through_its_program` (LiteSVM)
  - Kind: precondition
  - Statement: on the confidential and anonymous rails, every transfer with an unsigned account between the nullifier window and the settlement window returns Err.
  - Location: `programs/custom-ring/src/instructions/transfer/accounts.rs:257-261` (`fn check_owner_run`), `program-libs/interface/src/instructions/transfer_layout.rs:98-119` (`fn TransferLayout::pool`)
  - Error: `CustomRingError::MissingOwnerSigner = 8192`
  - Severity: Critical (spend authorization)
  - Suggested test: negative per run position, harness: LiteSVM

- [x] **INV-TRANSFER-37: a spending frame names its owner**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `a_spend_takes_at_least_one_owner_signer_up_to_the_key_slots`, `public_owners_settle_on_the_confidential_selector_with_any_destinations` (LiteSVM)
  - Kind: precondition
  - Statement: every frame with `no_spend` clear and an empty owner run returns Err on the confidential rail without an in-proof owner and on the anonymous rail under `PUBLIC_OWNERS`.
  - Location: `programs/custom-ring/src/instructions/transfer/accounts.rs:209-214` (`fn TransactCheck::validate`)
  - Error: `CustomRingError::OwnerSignerCount = 8208`
  - Severity: High
  - Suggested test: negative per rail, harness: LiteSVM

- [x] **INV-TRANSFER-38: the owner run fits the key's signer slots**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `an_owner_run_past_the_key_slots_is_refused` (LiteSVM)
  - Kind: precondition
  - Statement: every transfer whose owner run holds strictly more distinct addresses than the key's signer slots returns Err.
  - Location: `program-libs/interface/src/proof/statement.rs:266-278` (`fn SignerSet::new`), `programs/custom-ring/src/instructions/transfer/accounts.rs:271-273` (`fn signer_set`)
  - Error: `CustomRingError::OwnerSignerCount = 8208`
  - Severity: High
  - Suggested test: negative at slots + 1, harness: LiteSVM

- [x] **INV-TRANSFER-39: the statement's signer set is SPP's owner run**
  - Covered by: `program-libs/interface/tests/statements.rs` `the_signer_set_dedups_by_first_occurrence_and_fills_to_the_slots` (library), `program-tests/custom-ring/tests/rules/transact_rules.rs` `a_spend_takes_at_least_one_owner_signer_up_to_the_key_slots` (a repeated owner reaches the proof) (LiteSVM)
  - Kind: postcondition
  - Statement: the statement's `signers` element is exactly the count and the zero-filled chain of `solana_owner_identity` over the owner run deduplicated by first occurrence, in the account order SPP reads, zero on an empty run and on the delegate rails.
  - Location: `program-libs/interface/src/proof/statement.rs:266-299` (`fn SignerSet::new`, `fn zero_filled_chain`), `programs/custom-ring/src/instructions/transfer/accounts.rs:189-219` (`fn TransactCheck::validate`)
  - Severity: Critical (the proof holds each live owner to this set)
  - Suggested test: property over runs with repeats, harness: library

- [x] **INV-TRANSFER-40: the in-proof sender is the frame's owner key**
  - Covered by: `sdk-libs/client/tests/transact.rs` `the_in_proof_owner_proves_on_spp_anonymous_rail_under_the_sender_key` (library), `program-tests/custom-ring/tests/rules/transact_rules.rs` `an_in_proof_owner_signs_beside_a_co_owner_run` (LiteSVM)
  - Kind: postcondition
  - Statement: on the confidential rail with an in-proof owner, the statement's `sender` is exactly `solana_owner_identity` of the frame's owner key, and on every other frame it is zero.
  - Location: `programs/custom-ring/src/instructions/transfer/accounts.rs:200-216` (`fn TransactCheck::validate`), `program-libs/interface/src/proof/statement.rs:246-254` (`fn ConfidentialSender::identity`)
  - Severity: Critical
  - Suggested test: positive, harness: library

- [x] **INV-TRANSFER-41: the party cells are the co-signers on an anonymous frame**
  - Covered by: `sdk-libs/client/tests/anonymous_exit.rs` `the_co_signers_fill_the_cells_in_order`, `a_co_signer_past_the_fifth_is_refused` (library)
  - Kind: postcondition
  - Statement: on every anonymous frame, party cell `i` is exactly `solana_owner_identity` of the `i`-th verified co-signer and zero past the last one. On every other frame every cell is zero.
  - Location: `programs/custom-ring/src/instructions/transfer/accounts.rs:234-239` (`fn TransactCheck::validate`), `program-libs/interface/src/instructions/anonymous_exit.rs:65-77` (`fn anonymous_party_identities`)
  - Severity: Critical (the proof holds the hidden owner apart from these cells)
  - Suggested test: golden vector of the cells, harness: library

- [x] **INV-TRANSFER-42: every deposit leg's funder signs**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `public_legs_are_refused_with_the_rings_own_errors` (unsigned funder), `the_program_reads_each_signer_where_the_layout_names_it` (LiteSVM)
  - Kind: precondition
  - Statement: for every deposit leg, the account at the leg's funder position (1 for SOL, 2 for SPL) in its settlement group must sign, else the transfer returns Err.
  - Location: `programs/custom-ring/src/instructions/transfer/accounts.rs:181-186` (`fn TransactCheck::validate`), `programs/custom-ring/src/instructions/accounts/loader.rs:237-248` (`fn check_funding_signer`), `program-libs/interface/src/codec/spp.rs:238-244` (`fn funder_account_position`)
  - Error: `CustomRingError::MissingFundingSigner = 8173`
  - Severity: Critical (theft of a third party's balance)
  - Suggested test: negative per leg kind, harness: LiteSVM

### Settlement Window and Public Slots

- [x] **INV-TRANSFER-43: the settlement window ends SPP's list ahead of the cache accounts**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `public_legs_fit_the_account_list_and_the_leg_cap`, `a_delegate_transfer_refuses_short_or_overlong_spp_accounts_before_verification` (LiteSVM), `sdk-libs/client/tests/transfer_layout.rs` `the_windows_of_every_frame_tile_the_list`, `a_prefix_no_pair_count_fits_is_refused` (library)
  - Kind: precondition
  - Statement: every transfer whose list cannot hold the cache accounts and one settlement group per leg after the nullifier window returns Err.
  - Location: `program-libs/interface/src/instructions/transfer_layout.rs:98-119` (`fn TransferLayout::pool`)
  - Error: `CustomRingError::InvalidSettlementAccounts = 8242`
  - Severity: High
  - Suggested test: negative one account short, harness: LiteSVM

- [x] **INV-TRANSFER-44: the public slots fold the legs in SPP's order**
  - Covered by: `sdk-libs/client/tests/spp.rs` `public_amount_fields_match_the_spp_proof_inputs`, `public_slots_fold_the_gross_withdrawal_per_asset_without_its_deposits` (library)
  - Kind: postcondition
  - Statement: the statement's public slots are exactly the assets of the legs in first-seen order, SOL as `SOL_ASSET_FIELD` and an SPL mint as `hash_bytes(mint)`, each with the signed net (a withdrawal as `q - |amount|`) and the gross withdrawn, zero past the used slots.
  - Location: `programs/custom-ring/src/instructions/verification/public_legs.rs:11-27` (`fn public_slots`), `program-libs/interface/src/codec/spp.rs:78-176` (`fn PublicSlots::fold`, `fn asset_fields`, `fn amount_fields`, `fn credit`)
  - Severity: Critical (both proofs must name one asset per slot)
  - Suggested test: golden vector against SPP's `assign_public_amounts_and_assets`, harness: library

- [ ] **INV-TRANSFER-45: the public slot mints are the accounts SPP settles**
  - Partial coverage: `sdk-libs/client/tests/transfer_layout.rs` `every_window_holds_the_accounts_the_spp_builder_placed` (library) pins the window. No on-chain test swaps a mint account inside the window.
  - Kind: postcondition
  - Statement: for every SPL leg, the slot's mint is exactly the account at the leg's mint position inside its settlement group of the forwarded list.
  - Location: `programs/custom-ring/src/instructions/transfer/accounts.rs:163-164` (`fn TransactCheck::validate`), `program-libs/interface/src/codec/spp.rs:215-235` (`fn settlement_mints`)
  - Error: `CustomRingError::InvalidSettlementAccounts = 8242` (missing group)
  - Severity: Critical (asset substitution)
  - Suggested test: negative with a swapped mint account, harness: LiteSVM

- [x] **INV-TRANSFER-46: the public fold refuses what SPP's circuit cannot hold**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `public_legs_are_refused_with_the_rings_own_errors` (LiteSVM), `sdk-libs/client/tests/spp.rs` `an_intermediate_net_past_u64_is_refused_whatever_the_final_net` (library)
  - Kind: precondition
  - Statement: every transfer returns `TooManyPublicAssets` when its legs name more than 3 assets, `PublicAmountOverflow` when an asset's running net leaves the u64 magnitude at some step, and `ZeroNetPublicAmount` when an asset's final net is zero.
  - Location: `program-libs/interface/src/codec/spp.rs:78-91` (`fn PublicSlots::fold`), `spp.rs:161-176` (`fn PublicSlot::credit`), `program-libs/interface/src/error.rs:144-152` (`impl From<PublicSlotError>`)
  - Error: `CustomRingError::TooManyPublicAssets = 8226` / `CustomRingError::PublicAmountOverflow = 8227` / `CustomRingError::ZeroNetPublicAmount = 8228`
  - Severity: High
  - Suggested test: negative per leg, harness: LiteSVM

- [x] **INV-TRANSFER-47: a screened withdrawal names its live token owner and mint**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `policy_spl_metadata_must_match_the_live_token_owner` (LiteSVM)
  - Kind: precondition
  - Statement: on every `Own` or `Policy` read, every SPL withdrawal whose token account's mint differs from the leg's mint account, or whose token owner differs from the frame's owner entry, returns Err.
  - Location: `program-libs/interface/src/instructions/withdrawals.rs:73-93` (`fn policy_withdrawals`), `programs/custom-ring/src/instructions/transfer/accounts.rs:223-233` (`fn TransactCheck::validate`)
  - Error: `CustomRingError::InvalidSettlementAccounts = 8242`
  - Severity: Critical (a rule would screen an owner other than the paid one)
  - Suggested test: negative per field, harness: LiteSVM

- [ ] **INV-TRANSFER-48: a screened SPL withdrawal pays an initialized token program account**
  - Partial coverage: `sdk-libs/client/tests/anonymous_exit.rs` `policy_withdrawals_authenticate_live_owners_and_preserve_order` (uninitialized, empty, system-owned token accounts) and `an_account_reads_only_in_the_layouts_the_token_programs_write` (library) assert the refusals, the first only as `is_err`. No on-chain test.
  - Kind: precondition
  - Statement: on every `Own` or `Policy` read, every SPL withdrawal whose token account is not owned by SPL Token or Token-2022, does not unpack as a token account, or is not initialized, returns Err.
  - Location: `program-libs/interface/src/instructions/withdrawals.rs:74-80` (`fn policy_withdrawals`), `program-libs/interface/src/instructions/anonymous_exit.rs:38-55` (`fn TokenAccountState::unpack`), `programs/custom-ring/src/instructions/accounts/loader.rs:447-453` (`fn LoadedSettlement::token_account`)
  - Error: `CustomRingError::AnonymousExitTokenAccountInvalid = 8250`
  - Severity: High
  - Suggested test: negative per case, harness: LiteSVM

- [x] **INV-TRANSFER-49: the withdrawal hash binds every screened leg in order**
  - Covered by: `sdk-libs/client/tests/anonymous_exit.rs` `policy_withdrawals_authenticate_live_owners_and_preserve_order` (order and count move the hash), `withdrawal_digest_reads_u64_settlement_amounts` (the exact chain), `policy_deposits_are_not_screened_and_zero_withdrawals_are_refused` (library), `program-tests/custom-ring/tests/ring/one_key.rs` `one_key_settles_a_real_transfer_and_refuses_three_forgeries` (localnet)
  - Kind: postcondition
  - Statement: the statement's `withdrawal_hash` is exactly `HashChain4(WITHDRAWALS, count, then owner, asset and amount of each withdrawal leg in leg order)`. Deposit legs take no cell, and a `None` read hashes a count of 0 and no leg.
  - Location: `program-libs/interface/src/instructions/withdrawals.rs:25-39` (`fn policy_withdrawal_hash`), `programs/custom-ring/src/instructions/transfer/verify.rs:51-53` (`fn RingStatements::verify`)
  - Severity: Critical
  - Suggested test: golden vector with two legs to one owner, harness: library

### Proof and CPI

- [x] **INV-TRANSFER-50: the frame and the screened legs select the ring key**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `a_default_ring_spend_takes_the_entry_key_up_to_eight_inputs`, `a_compact_transfer_reaches_the_key_of_its_capacity`, `the_vector_transfers_without_public_legs_reach_their_keys` (LiteSVM)
  - Kind: precondition
  - Statement: the ring proof verifies only under the catalog key whose shape holds the frame's rail, nullifier count, output width, entry flag, wide flag and screened withdrawal count. A frame no key holds returns Err.
  - Location: `programs/custom-ring/src/instructions/transfer/verify.rs:72-77` (`fn RingStatements::verify`), `program-libs/interface/src/instructions/transact.rs:960-973` (`fn transfer_circuit`), `program-libs/interface/src/proof/key_catalog.rs:153-159` (`fn RingCircuit::transfer`)
  - Error: `CustomRingError::RingShapeUnkeyed = 8201`
  - Severity: Critical
  - Suggested test: positive per catalog shape, negative past it, harness: LiteSVM

- [x] **INV-TRANSFER-51: the statement takes no ring, auditor or root value from the frame**
  - Covered by: `program-tests/custom-ring/tests/ring/one_key.rs` `one_key_settles_a_real_transfer_and_refuses_three_forgeries` (the program's statement equals the prover's, an altered statement fails) (localnet), `program-libs/interface/tests/statements.rs` `ring_statements_vector` (library)
  - Kind: postcondition
  - Statement: the statement's `ring_id` is exactly `hash_bytes(program_id)`, `auditor_pubkey` is exactly the ring account's key, and every root and the policy hash are the values of INV-TRANSFER-23. No frame byte reaches these elements.
  - Location: `programs/custom-ring/src/instructions/transfer/verify.rs:44-70` (`fn RingStatements::verify`), `program-libs/interface/src/instructions/transact.rs:577-628` (`fn ParsedTransact::transfer_statement`)
  - Severity: Critical (a frame value would seal the audit to a key the prover holds)
  - Suggested test: negative per element with a real proof, harness: localnet

- [x] **INV-TRANSFER-52: the ring proof verifies before `ring_auth` signs**
  - Covered by: `program-tests/custom-ring/tests/ring/one_key.rs` `one_key_settles_a_real_transfer_and_refuses_three_forgeries` (an altered record fails with 8101 and the tree is unchanged) (localnet), every `reaches_verification` case in `program-tests/custom-ring/tests/rules/transact_rules.rs` (LiteSVM)
  - Kind: precondition
  - Statement: every transfer whose ring proof does not verify over the rebuilt statement returns Err, and no SPP CPI happens in it.
  - Location: `programs/custom-ring/src/instructions/transfer/processor.rs:60-69` (`fn process_transfer_ix`)
  - Error: `CustomRingError::ProofVerificationFailed = 8101` / `CustomRingError::InvalidRingProofPoint = 8238`
  - Severity: Critical (a CPI ahead of the proof would settle a transfer no ring rule covered)
  - Suggested test: negative with a valid SPP proof and a wrong ring proof, harness: localnet

- [x] **INV-TRANSFER-53: the CPI data is the frame's exact SPP expansion**
  - Covered by: `sdk-libs/client/tests/transact.rs` `ring_transact_expansion_vector`, `the_extensions_reach_the_forwarded_data`, `the_external_data_hash_binds_every_delivery_byte`, `a_tag_on_the_wire_reaches_the_external_data_hash_at_its_width` (library)
  - Kind: postcondition
  - Statement: the SPP instruction data is exactly the rail's SPP tag followed by the wincode `TransactIxData` with the expiry widened to u64, `tx_viewing_pk` as `0x04 || u`, every output as an `Inline` owner tag with its encoded body and reply, the application messages, and last the auditor message under the auditor key's `u` as its view tag.
  - Location: `program-libs/interface/src/instructions/transact/expansion.rs:54-108` (`fn spp_instruction_data_validated`, `fn transact_ix_data_validated`), `expansion.rs:133-135` (`fn auditor_tag`)
  - Severity: Critical (SPP's external data hash and the ring statement must read the same bytes)
  - Suggested test: golden vector per rail, harness: library

- [x] **INV-TRANSFER-54: the SPP selector follows the owner form**
  - Covered by: `sdk-libs/client/tests/transact.rs` `public_owners_select_the_confidential_circuit`, `a_cached_selector_keeps_the_rail_key`, `the_in_proof_owner_proves_on_spp_anonymous_rail_under_the_sender_key` (library), `program-tests/custom-ring/tests/rules/transact_rules.rs` `public_owners_settle_on_the_confidential_selector_with_any_destinations` (LiteSVM)
  - Kind: postcondition
  - Statement: the forwarded circuit is exactly `AnonymousRingEddsa` with the owner's commitment for an in-proof owner, `ConfidentialRingEddsa` for `PUBLIC_OWNERS` and for the confidential signer rail, `AnonymousRingEddsa` with the hidden owner's data for a hidden sender, `RingAuthority` for both delegate rails, and the cached twin when the frame carries a cache selector.
  - Location: `program-libs/interface/src/instructions/transact.rs:1002-1015` (`fn RingTransactData::circuit`), `transact.rs:306-321` (`fn RailAuthorization::circuit`), `transact.rs:167-176` (`fn RingRail::circuit`)
  - Severity: Critical (the selector decides which owner SPP authorizes)
  - Suggested test: positive per rail, harness: library

- [ ] **INV-TRANSFER-55: a successful transfer calls SPP once and writes no account itself**
  - Partial coverage: `program-tests/custom-ring/tests/ring/transfers.rs` `auditor_sees_every_ring_transfer` and `program-tests/custom-ring/tests/ring/one_key.rs` `one_key_settles_a_real_transfer_and_refuses_three_forgeries` (localnet) assert the settled notes. No test asserts a single inner instruction or the unchanged ring-owned accounts.
  - Kind: postcondition
  - Statement: after every successful transfer, the transaction holds exactly one inner instruction of the ring program, an SPP call with the data of INV-TRANSFER-53 over the forwarded list, and the ring program wrote no account.
  - Location: `programs/custom-ring/src/instructions/transfer/processor.rs:25-70` (`fn process_transfer_ix`), `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs:148-184` (`fn PlacedRingAuth::invoke`, `fn invoke_program`)
  - Severity: High
  - Suggested test: positive with inner instruction inspection, harness: LiteSVM with an SPP stand-in

- [ ] **INV-TRANSFER-56: a transfer changes no ring-owned account**
  - Not covered. No test compares the ring account, the co-signer records or the registries before and after a transfer.
  - Kind: frame
  - Statement: after every transfer, the ring account, every co-signer record, every registry and the delegate and mint accounts in the prefix are unchanged in data and lamports.
  - Location: `programs/custom-ring/src/instructions/transfer/processor.rs:25-70` (`fn process_transfer_ix`)
  - Severity: High
  - Suggested test: positive with an account snapshot, harness: localnet

## AuthorityTransfer

### Authorization

- [x] **INV-AUTH-TRANSFER-01: the delegate slot signs**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `only_the_configured_delegate_moves_withdraws_and_takes_no_deposit` (unsigned delegate) (LiteSVM)
  - Kind: precondition
  - Statement: on both delegate rails, every call whose account after the pair window is not a signer returns Err.
  - Location: `programs/custom-ring/src/instructions/transfer/accounts.rs:100-105` (`fn LivePrefix::delegate_mint`)
  - Error: `CustomRingError::UnauthorizedDelegate = 8151`
  - Severity: Critical
  - Suggested test: negative, harness: LiteSVM

- [x] **INV-AUTH-TRANSFER-02: only the ring's delegate or a mint's live permanent delegate moves notes**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `only_the_configured_delegate_moves_withdraws_and_takes_no_deposit`, `a_mint_names_its_live_permanent_delegate_on_the_authority_rail` (LiteSVM)
  - Kind: precondition
  - Statement: a delegate signer that is not the ring's delegate passes only when the next account is a Token-2022 mint whose permanent delegate extension names that signer. Every other signer returns `UnauthorizedDelegate` on a ring with a delegate and `DelegateDisabled` on a ring without one.
  - Location: `programs/custom-ring/src/instructions/transfer/accounts.rs:106-119` (`fn LivePrefix::delegate_mint`), `programs/custom-ring/src/instructions/accounts/loader.rs:420-434` (`fn load_mint_delegate_asset`), `program-libs/interface/src/instructions/transact.rs:105-117` (`fn MintDelegate::asset`)
  - Error: `CustomRingError::UnauthorizedDelegate = 8151` / `CustomRingError::DelegateDisabled = 8150`
  - Severity: Critical (authority takeover over every note)
  - Suggested test: negative per mint state, harness: LiteSVM

- [x] **INV-AUTH-TRANSFER-03: the delegate asset names the mint whose delegate signs**
  - Covered by: `sdk-libs/client/tests/transact.rs` `a_mint_names_its_asset_only_to_its_live_permanent_delegate` (library)
  - Kind: postcondition
  - Statement: the statement's `delegate_asset` is exactly `hash_bytes(mint)` when a mint's permanent delegate signs and zero when the ring's delegate signs or on every non-delegate rail.
  - Location: `programs/custom-ring/src/instructions/transfer/accounts.rs:109-114` (`fn LivePrefix::delegate_mint`), `program-libs/interface/src/instructions/transact.rs:116` (`fn MintDelegate::asset`)
  - Severity: Critical (the proof holds every note to this asset)
  - Suggested test: golden vector, harness: library

- [x] **INV-AUTH-TRANSFER-04: a delegate move takes no deposit leg**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `only_the_configured_delegate_moves_withdraws_and_takes_no_deposit` (LiteSVM)
  - Kind: precondition
  - Statement: on both delegate rails, every frame with a deposit leg returns Err.
  - Location: `programs/custom-ring/src/instructions/transfer/accounts.rs:171-178` (`fn TransactCheck::validate`)
  - Error: `CustomRingError::DelegatePublicLeg = 8152`
  - Severity: High (a deposit would bring an unlisted asset in)
  - Suggested test: negative per deposit kind, harness: LiteSVM

- [x] **INV-AUTH-TRANSFER-05: a delegate move names no owner signer**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `a_delegate_transfer_refuses_short_or_overlong_spp_accounts_before_verification` (LiteSVM)
  - Kind: precondition
  - Statement: on both delegate rails, every call with a nonempty owner run returns Err.
  - Location: `programs/custom-ring/src/instructions/transfer/accounts.rs:190-197` (`fn TransactCheck::validate`)
  - Error: `CustomRingError::InvalidSettlementAccounts = 8242`
  - Severity: High
  - Suggested test: negative, harness: LiteSVM

- [x] **INV-AUTH-TRANSFER-06: a delegate move demands no co-signer**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `a_delegate_ignores_every_co_signer_record_and_carries_none` (LiteSVM)
  - Kind: state
  - Statement: on both delegate rails the co-sign demand is empty for every ring configuration, so the account after the registries is the delegate slot.
  - Location: `program-libs/interface/src/instructions/transact.rs:984-986` (`fn co_sign_requirement`)
  - Severity: Medium
  - Suggested test: positive with a covering record, harness: LiteSVM

- [x] **INV-AUTH-TRANSFER-07: a delegate move reads the tracking root alone**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `a_delegate_reads_no_policy_root_and_keeps_the_tracking_root` (LiteSVM)
  - Kind: precondition
  - Statement: on both delegate rails the frame names only a tracking revision, a restrictive policy update leaves an in-flight delegate proof valid, and a tracking revision the registry does not serve returns Err.
  - Location: `program-libs/interface/src/accounts/state.rs:427-433` (`fn RootRead::transfer`), `program-libs/interface/src/instructions/transact.rs:917-958` (`fn roots`)
  - Error: `CustomRingError::RootNotServed = 8481`
  - Severity: High
  - Suggested test: negative with a stale tracking revision, harness: LiteSVM

- [x] **INV-AUTH-TRANSFER-08: every SPP ring-authority shape reaches its key**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `every_ring_authority_shape_reaches_the_delegate_key_lookup`, `a_delegate_shape_without_a_ring_authority_key_is_refused` (LiteSVM)
  - Kind: reachability
  - Statement: for every SPP shape the ring-authority selector supports, a delegate move reaches the ring proof check, and every other shape returns Err.
  - Location: `program-libs/interface/src/instructions/transact.rs:178-187` (`fn RingRail::supports`)
  - Error: `CustomRingError::RingShapeUnkeyed = 8201`
  - Severity: Medium
  - Suggested test: positive per shape, harness: LiteSVM

- [ ] **INV-AUTH-TRANSFER-09: the delegate statement carries the delegate rail bit**
  - Partial coverage: `program-libs/interface/tests/statements.rs` `the_element_count_follows_the_family_alone` (library) covers the family size. No test pins the rail bits of the two delegate families.
  - Kind: postcondition
  - Statement: the statement's rail bits are exactly `[0, 0, 1]` on both delegate rails, `[1, 0, 0]` on the in-proof rail, `[0, 1, 0]` for a hidden sender and `[0, 0, 0]` otherwise.
  - Location: `program-libs/interface/src/proof/statement.rs:214-227` (`fn TransferFamily::rail_bits`), `program-libs/interface/src/instructions/transact.rs:583-598` (`fn transfer_statement`)
  - Severity: Critical (only the delegate bit parts the signer and delegate rails in the circuit)
  - Suggested test: golden vector per rail, harness: library

## ApproveTransfer

### Authorization

- [x] **INV-APPROVE-01: an approval runs only at the top level**
  - Covered by: `program-tests/custom-ring/tests/rules/program_owner_rules.rs` `an_approval_runs_only_at_the_top_level_under_its_co_signer` (LiteSVM)
  - Kind: precondition
  - Statement: every `ApproveTransfer` whose stack height is not exactly 1 returns Err before it reads an account.
  - Location: `programs/custom-ring/src/instructions/transfer/approve_transfer.rs:21-23` (`fn process_approve_transfer_ix`), `approve_transfer.rs:43-51` (`fn stack_height`)
  - Error: `CustomRingError::ApprovalNotTopLevel = 8273`
  - Severity: High (the sysvar lists top-level instructions alone)
  - Suggested test: negative through a forwarder, harness: LiteSVM

- [x] **INV-APPROVE-02: the record's signer signs the approval**
  - Covered by: `program-tests/custom-ring/tests/rules/program_owner_rules.rs` `an_approval_runs_only_at_the_top_level_under_its_co_signer` (LiteSVM)
  - Kind: precondition
  - Statement: every `ApproveTransfer` whose third account is unsigned returns `MissingCoSigner`, and every one whose third account differs from the record's `signer` returns `UnauthorizedCoSigner`.
  - Location: `programs/custom-ring/src/instructions/transfer/approve_transfer.rs:33-38` (`fn process_approve_transfer_ix`)
  - Error: `CustomRingError::MissingCoSigner = 8143` / `CustomRingError::UnauthorizedCoSigner = 8144`
  - Severity: High
  - Suggested test: negative both legs, harness: LiteSVM

### Account Constraints and Data

- [ ] **INV-APPROVE-03: the approval names the ring account and a co-signer record**
  - Not covered. No test sends an `ApproveTransfer` with another ring account or another record.
  - Kind: precondition
  - Statement: every `ApproveTransfer` whose first account is not the program's `RingPolicy` returns `InvalidRingPolicy`, and every one whose second account is not a program `CoSigner` returns `InvalidCoSignerRecord`.
  - Location: `programs/custom-ring/src/instructions/transfer/approve_transfer.rs:31-32` (`fn process_approve_transfer_ix`)
  - Error: `CustomRingError::InvalidRingPolicy = 8300` / `CustomRingError::InvalidCoSignerRecord = 8260`
  - Severity: Medium (the transfer repeats every identity check, INV-TRANSFER-30..33)
  - Suggested test: negative both legs, harness: LiteSVM

- [ ] **INV-APPROVE-04: the approval data is exactly a position and a digest**
  - Not covered. No test sends a short or long body.
  - Kind: precondition
  - Statement: every `ApproveTransfer` whose data after the tag is not exactly 33 bytes returns Err.
  - Location: `programs/custom-ring/src/instructions/transfer/approve_transfer.rs:24` (`fn process_approve_transfer_ix`), `program-libs/interface/src/instructions/transfer_approval.rs:67-75` (`fn TransferApproval::parse_body`)
  - Error: `CustomRingError::InvalidInstructionData = 8100`
  - Severity: Medium
  - Suggested test: negative at 32 and 34 bytes, harness: LiteSVM

### Success and Frame

- [x] **INV-APPROVE-05: a valid approval succeeds**
  - Covered by: `program-tests/custom-ring/tests/rules/program_owner_rules.rs` `an_approval_runs_only_at_the_top_level_under_its_co_signer` (`within_budget`), `a_top_level_approval_co_signs_the_transfer_a_program_forwards` (LiteSVM)
  - Kind: postcondition
  - Statement: every top-level `ApproveTransfer` over the ring account, a record and that record's signing signer returns Ok for every 33-byte body.
  - Location: `programs/custom-ring/src/instructions/transfer/approve_transfer.rs:15-40` (`fn process_approve_transfer_ix`)
  - Severity: Medium
  - Suggested test: positive, harness: LiteSVM

- [ ] **INV-APPROVE-06: an approval writes nothing**
  - Not covered. No test compares accounts before and after an approval.
  - Kind: frame
  - Statement: after every `ApproveTransfer`, every account of the transaction is unchanged in data and lamports by it.
  - Location: `programs/custom-ring/src/instructions/transfer/approve_transfer.rs:15-40` (`fn process_approve_transfer_ix`)
  - Severity: Medium
  - Suggested test: positive with an account snapshot, harness: LiteSVM
