# Cross-Cutting Invariants

Invariants that hold for more than one instruction. Each entry lists the instructions it affects. The instruction files reference these IDs and do not repeat them.

## Dispatch

- [ ] **INV-XC-01: empty ring instruction data is refused**
  - Not covered. No test sends a ring instruction with no data.
  - Kind: precondition
  - Affects: all 12 ring instructions
  - Statement: the ring program returns Err for zero-length instruction data.
  - Location: `programs/custom-ring/src/lib.rs:79-81` (`fn process_instruction`)
  - Error: `ProgramError::InvalidInstructionData` (built-in)
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM

- [ ] **INV-XC-02: every unknown ring tag is refused, every known tag reaches its processor**
  - Partial coverage: `program-tests/custom-ring/tests/rules/operator_rules.rs` `unknown_tags_are_refused` (24 bytes), `program-tests/custom-ring/tests/rules/transact_rules.rs` `the_program_names_no_instruction_at_tag_58` (LiteSVM), `sdk-libs/client/tests/tags.rs` `unassigned_tags_name_no_instruction`, `every_tag_names_one_instruction_and_its_budget` (library). No on-chain sweep over all 256 first bytes.
  - Kind: precondition
  - Affects: all 12 ring instructions
  - Statement: for every first byte outside {6, 11, 27, 53, 54, 55, 56, 57, 62, 64, 65, 66} the ring program returns Err, and every byte inside the set runs exactly the processor of its tag.
  - Location: `programs/custom-ring/src/lib.rs:82-103` (`fn process_instruction`), `program-libs/interface/src/instructions/instruction.rs:30-60` (`impl TryFrom<u8> for InstructionTag`)
  - Error: `CustomRingError::InvalidInstructionData = 8100`
  - Severity: Medium
  - Suggested test: property over all 256 first bytes, harness: LiteSVM

- [x] **INV-XC-03: no ring tag equals a forwarded SPP tag**
  - Covered by: `sdk-libs/client/tests/tags.rs` `ring_dispatch_tags_do_not_collide_with_forwarded_spp_tags`, `frame_tags_stay_pinned` (library), `program-tests/custom-ring/tests/rules/operator_rules.rs` `an_spp_tag_with_its_forwarded_body_refuses_at_the_ring` (LiteSVM)
  - Kind: state
  - Affects: Transfer, AuthorityTransfer, Merge
  - Statement: no ring instruction tag equals `RING_TRANSACT`, `RING_MERGE_TRANSACT` or `RING_AUTHORITY_TRANSACT`, so a forwarded SPP body sent to the ring program is refused.
  - Location: `program-libs/interface/src/instructions/instruction.rs:43-60` (`instruction_tags!`)
  - Error: `CustomRingError::InvalidInstructionData = 8100`
  - Severity: Medium
  - Suggested test: unit over the tag table, harness: library

- [ ] **INV-XC-04: every unknown storage tag and empty storage data is refused**
  - Partial coverage: `program-tests/custom-ring/tests/storage_lifecycle.rs` `unknown_tags_are_refused` (tags 2, 3, 5, 6, 11) (LiteSVM storage). No test sends empty data or sweeps every first byte.
  - Kind: precondition
  - Affects: all 7 storage instructions
  - Statement: for zero-length data and for every first byte outside {0, 1, 4, 7, 8, 9, 10} the storage program returns Err.
  - Location: `programs/storage/src/lib.rs:51-64` (`fn process_instruction`)
  - Error: `StorageError::InvalidInstructionData = 9005`
  - Severity: Medium
  - Suggested test: property over all 256 first bytes, harness: LiteSVM storage

- [ ] **INV-XC-05: one ring per deployment**
  - Partial coverage: `program-tests/custom-ring/tests/rules/operator_rules.rs` `initialize_takes_its_canonical_ring_address_once_beside_the_system_program` (LiteSVM) refuses a second `Initialize`. No test shows a second deployment's ring account refused by the first deployment.
  - Kind: state
  - Affects: all 12 ring instructions
  - Statement: every ring-owned account the program reads is owned by the invoked program id, the ring account sits at the single PDA of `[b"policy"]`, and `ring_auth` at the single PDA of `[RING_AUTH_PDA_SEED]`, so no deployment reads another deployment's ring.
  - Location: `programs/custom-ring/src/instructions/accounts/loader.rs:38-94` (`fn load`, `fn load_mut`, `struct OwnedAccount`), `program-libs/interface/src/accounts/state.rs:112-119` (`ring_accounts!` seeds), `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs:112-116` (`fn RingAuth::derive`)
  - Error: `CustomRingError::InvalidRingPolicy = 8300` (another program's ring account)
  - Severity: Critical
  - Suggested test: negative with a second deployment's ring account, harness: LiteSVM

## Instruction Data

- [ ] **INV-XC-06: every ring instruction parses its data exactly**
  - Partial coverage: `program-tests/custom-ring/tests/rules/operator_rules.rs` `the_delegate_signs_its_appointment_and_no_data_names_it` (a trailing byte on `SetDelegate`), `update_authority_needs_the_new_keys_signature` (an unknown role), `each_ring_kind_fixes_its_policy_flag_and_mode` (an unknown kind) (LiteSVM). No test sends a trailing byte to `Initialize`, `UpdatePolicy`, `SetPaused`, `SetCoSigner` or `ClearCoSigner`.
  - Kind: precondition
  - Affects: Initialize, UpdateAuthority, UpdatePolicy, SetPaused, SetCoSigner, ClearCoSigner, SetDelegate
  - Statement: every wincode body with a trailing byte or a field off its encoding returns Err, and every no-data instruction with a data byte after the tag returns Err.
  - Location: `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs:24-35` (`fn parse`, `fn no_data`)
  - Error: `CustomRingError::InvalidInstructionData = 8100`
  - Severity: Medium
  - Suggested test: negative per instruction, harness: LiteSVM

- [ ] **INV-XC-07: every fixed storage body has its exact size**
  - Not covered.
  - Kind: precondition
  - Affects: storage Initialize, InitializeTracking, OwnerGrant, UpdateAuthority
  - Statement: every fixed-layout storage body whose length differs from its struct size returns Err, and `UpdateAuthority` with a data byte returns Err.
  - Location: `programs/storage/src/lib.rs:82-84` (`fn parse`), `programs/storage/src/instructions/update_authority.rs:17-19` (`fn process_update_authority_ix`)
  - Error: `StorageError::InvalidInstructionData = 9005`
  - Severity: Medium
  - Suggested test: negative at size minus 1 and plus 1, harness: LiteSVM storage

## Atomicity and Rollback

- [ ] **INV-XC-08: every failing instruction leaves every account unchanged**
  - Partial coverage: `program-tests/custom-ring/tests/rules/salted_source_rules.rs` `refused_unchanged` cases (UpdatePolicy), the `before == after` cases in `program-tests/custom-ring/tests/storage_lifecycle.rs` (Update, ApplyEnrollments), the ring PDA lamport checks in `program-tests/custom-ring/tests/rules/operator_rules.rs` (Initialize) (LiteSVM), `program-tests/custom-ring/tests/ring/one_key.rs` `one_key_settles_a_real_transfer_and_refuses_three_forgeries` (the SPP tree after a refused transfer) (localnet). No instruction has a full account-equality sweep on refusal.
  - Kind: rollback
  - Affects: all 12 ring and all 7 storage instructions
  - Statement: when an instruction of either program returns Err, every account of the transaction holds the data and lamports it held before.
  - Location: `programs/custom-ring/src/lib.rs:74-104`, `programs/storage/src/lib.rs:46-65` (`fn process_instruction`)
  - Severity: Critical
  - Suggested test: negative per instruction with a full snapshot compare, harness: LiteSVM

- [x] **INV-XC-09: a write ahead of a later refusal does not persist**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `initialize_refuses_a_tracking_account_off_its_seeds` (the ring account written, then storage refuses) (LiteSVM), `program-tests/custom-ring/tests/storage_lifecycle.rs` `signed_rotations_reach_the_apply_proof_under_the_registry_authority` (the root written, then the proof fails) (LiteSVM storage)
  - Kind: rollback
  - Affects: Initialize (the ring account precedes both CPIs), ApplyEnrollments (the root precedes the proof), Transfer, AuthorityTransfer, Merge, SetPaused, EnrollBatch (a refusal inside the CPI)
  - Statement: when a step after a write returns Err, including a refusal inside a CPI, the written account equals its value before the instruction.
  - Location: `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:133-162` (`fn process_initialize_ix`), `programs/storage/src/instructions/apply_enrollments.rs:85-109` (`fn process_apply_enrollments_ix`)
  - Severity: Critical
  - Suggested test: negative per write and later refusal, harness: LiteSVM

## Error Codes

- [x] **INV-XC-10: ring error codes are stable**
  - Covered by: `program-libs/interface/tests/error_codes.rs` `error_codes_are_stable` (library)
  - Kind: state
  - Affects: all 12 ring instructions
  - Statement: every `CustomRingError` discriminant equals its pinned code, the pin is a compiler-exhaustive match, and the shared fixture `test-vectors/errors/custom-ring-errors.json` names exactly the same 82 variants and codes.
  - Location: `program-libs/interface/src/error.rs:19-105` (`ring_errors!`)
  - Severity: Medium (client ABI)
  - Suggested test: none remaining, harness: library

- [x] **INV-XC-11: storage error codes are stable**
  - Covered by: `program-libs/interface/tests/error_codes.rs` `storage_error_codes_are_stable` (library)
  - Kind: state
  - Affects: all 7 storage instructions
  - Statement: every `StorageError` discriminant listed in the pin equals its code, and the shared fixture `test-vectors/errors/storage-errors.json` names exactly the same 29 variants and codes.
  - Location: `program-libs/interface/src/accounts/storage.rs:307-368` (`enum StorageError`)
  - Severity: Medium (client ABI)
  - Suggested test: make the pin a compiler-exhaustive match, so a new variant fails to compile until pinned, harness: library

- [ ] **INV-XC-12: library errors convert to fixed ring codes**
  - Partial coverage: on-chain refusals in `program-tests/custom-ring/tests/rules/operator_rules.rs` `initialize_refuses_each_table_form`, `a_rule_over_a_member_written_list_is_refused`, `a_table_write_decodes_and_counts_its_generation_on_the_policy`, `policy_writes_need_a_policy_ring_and_a_foreign_curator` (LiteSVM) show `MultiListAbsenceRule`, `InvertedListPolarity`, `MixedPolarityRule`, `MemberWrittenListRule`, `InvalidPolicyRules` and `InvalidSource`. No table test pins each conversion, and `ListNotPolicyReadable` appears in no refusal test.
  - Kind: state
  - Affects: Initialize, UpdatePolicy, Transfer, AuthorityTransfer, Merge
  - Statement: `RuleTableError` converts exactly as `MemberWrittenList` to 8233, `ListNotPolicyReadable` to 8255, `MultiListAbsence` to 8235, `InvertedPolarity` to 8236, `MixedPolarity` to 8237 and every other variant to 8140. `SourceMapError` and `PolicyTableError::MissingSource` convert to 8134, `AssetLimitsError` to 8390, `HasherError` to 8102, and `PublicSlotError` to 8226, 8227 or 8228 by variant.
  - Location: `program-libs/interface/src/error.rs:113-161` (the `From` impls)
  - Severity: Medium
  - Suggested test: table test over every source variant, harness: library

- [ ] **INV-XC-13: account iterator refusals keep their codes**
  - Partial coverage: `program-tests/custom-ring/tests/rules/operator_rules.rs` `the_delegate_signs_its_appointment_and_no_data_names_it`, `update_authority_needs_the_new_keys_signature` (`InvalidSigner`) (LiteSVM). No test covers `NotEnoughAccountKeys` or `AccountNotMutable`.
  - Kind: state
  - Affects: Initialize, UpdateAuthority, UpdatePolicy, SetPaused, SetCoSigner, ClearCoSigner, SetDelegate, ApproveTransfer, EnrollBatch, Merge
  - Statement: a missing account returns `AccountError::NotEnoughAccountKeys` (20014), an unsigned signer slot `AccountError::InvalidSigner` (20009) and a read-only writable slot `AccountError::AccountNotMutable` (20002). `Transfer`, `AuthorityTransfer` and the merge's forwarded list return the built-in `ProgramError::NotEnoughAccountKeys` for a missing account.
  - Location: `zolana/program-libs/account-checks/src/account_iterator.rs:142-235`, `programs/custom-ring/src/instructions/transfer/processor.rs:35, 54` (`fn process_transfer_ix`)
  - Severity: Medium
  - Suggested test: negative per slot kind, harness: LiteSVM

- [-] **INV-XC-14: ring error variants no program path returns**
  - Not reachable on chain. The entry pins the fact so a test author does not chase them.
  - Kind: reachability
  - Affects: none
  - Statement: no path of the ring program or the storage program returns these `CustomRingError` codes:
    1. `InvalidRingMode` (8174): `load` refuses an unknown mode as `InvalidRingPolicy` through `RingPolicy::valid` before `RingRail::of` reads the mode (`program-libs/interface/src/accounts/state.rs:112-115`, `programs/custom-ring/src/instructions/transfer/processor.rs:33-37`). The library path is covered by `sdk-libs/client/tests/accounts.rs` and `sdk-libs/client/tests/transact.rs` `the_rail_follows_the_instruction_and_the_ring_mode`.
    2. `UnsupportedCircuit` (8113): every catalog circuit has a compiled verifying key with a commitment (10 transfer keys, the merge key, `program-libs/interface/src/verifying_keys/mod.rs:161-300`). The leg is covered by `program-libs/interface/src/proof/verification.rs` `each_proof_stage_fails_with_its_own_code` with a key built without a commitment.
    3. `CoSignerRecordAlreadyExists` (8261): `SetCoSigner` creates a record only over an empty account (`programs/custom-ring/src/instructions/configuration/access/set_cosigner.rs:46-62`).
    4. `TooManyAnonymousParties` (8528) and `TooManyAccounts` from `CoSigners::verify`: each pair covers a new class of 5, so at most 5 pairs exist (`program-libs/interface/src/instructions/cosign.rs:24-26`).
    5. `MemberOutOfRange` (8209), `ListBitNotWritable` (8210), `ValueOutsideMask` (8211), `AuthorityBatchTooLarge` (8214), `InvalidAssetLimits` (8390): the ring program parses no record write, and the storage program maps them to `StorageError::InvalidInstructionData` and `BatchTooLarge` (`program-libs/interface/src/instructions/storage.rs:404, 514-519`).
    6. `ZeroPublicLegAmount` from `policy_withdrawals` and `InvalidSettlementAccounts` from its leftover-group check: `check_public_legs` and `TransferLayout::pool` refuse both cases first (`program-libs/interface/src/instructions/withdrawals.rs:95-97, 116-118`).
    7. `InvalidInstructionData` from `RingRail::of`: dispatch passes only tags 55 and 56 (`programs/custom-ring/src/lib.rs:86-88`).
  - Location: `program-libs/interface/src/error.rs:19-105` (`ring_errors!`)
  - Severity: Medium (error-surface hygiene)
  - Suggested test: none on chain, keep the library tests

- [-] **INV-XC-15: storage error legs no input reaches**
  - Not reachable on chain, or only after an infeasible count of writes.
  - Kind: reachability
  - Affects: storage Update, ApplyEnrollments
  - Statement: no feasible input reaches these legs. `HashingFailed` (9009) is not among them, `INV-XC-48` names its trigger.
    1. `RevisionOverflow` (9015) needs revision `u64::MAX` (`program-libs/interface/src/accounts/storage.rs:264-267`). Covered at the library level by `sdk-libs/client/tests/accounts.rs` `storage_update_refuses_revision_overflow`.
    2. `UnsupportedBatch` (9016) from a missing apply key: every apply width of both modes and every enrollment width is keyed (`program-libs/interface/src/verifying_keys/mod.rs:245-296`). The leg that stays reachable is the PDA path with a count other than 1, `INV-ST-APPLY-08`.
    3. `PrivateWriteWithoutTracking` (9035) inside `StorageRegistry::update` and `RevisionChanged` (9002) inside `advance_roots` on the `Update` path: `process_update_ix` checks both first (`programs/storage/src/instructions/update.rs:46-61`).
  - Location: `program-libs/interface/src/accounts/storage.rs:307-368` (`enum StorageError`)
  - Severity: Medium
  - Suggested test: none on chain

## Roles

- [x] **INV-XC-16: a role-gated instruction accepts exactly its role's key**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `each_role_instruction_refuses_every_other_role_and_the_upgrade_authority` (8 writes, every other role and the upgrade authority), `only_the_enrollment_writer_reaches_storage_through_the_ring_pda` (LiteSVM)
  - Kind: precondition
  - Affects: UpdateAuthority, SetDelegate (Admin), UpdatePolicy (Policy), SetPaused (Pauser), SetCoSigner, ClearCoSigner (CosignAdmin), EnrollBatch (EnrollmentWriter)
  - Statement: every role-gated instruction returns Err unless its signer slot holds exactly the ring account's key for its role, and the upgrade authority holds no role it was not given.
  - Location: `programs/custom-ring/src/instructions/accounts/loader.rs:98-121` (`struct RoleHolder`), `program-libs/interface/src/accounts/state.rs:235-240` (`fn check_role`)
  - Error: `CustomRingError::UnauthorizedAuthority = 8107`
  - Severity: Critical
  - Suggested test: negative per (instruction, other role), harness: LiteSVM

- [ ] **INV-XC-17: role keys are nonzero and apart from the delegate**
  - Partial coverage: `program-tests/custom-ring/tests/rules/operator_rules.rs` `initialize_refuses_a_zero_role_key_and_takes_a_shared_one`, `the_delegate_is_permanent_and_any_ring_takes_one` (LiteSVM), `sdk-libs/client/tests/accounts.rs` `role_keys_stay_nonzero_and_apart_from_the_delegate` (library). No on-chain test rotates a role to the delegate.
  - Kind: state
  - Affects: Initialize, UpdateAuthority, SetDelegate
  - Statement: after every successful ring instruction, all five role keys are nonzero, and when a delegate is set it is nonzero and differs from every role key. One key may hold several roles.
  - Location: `program-libs/interface/src/accounts/state.rs:541-560` (`fn RingRoles::check`), called from `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:77`, `state.rs:243-249` (`fn rotate_role`), `programs/custom-ring/src/instructions/configuration/access/set_delegate.rs:43`
  - Error: `CustomRingError::ZeroRoleKey = 8471` / `CustomRingError::DuplicateRoleKey = 8472`
  - Severity: High
  - Suggested test: property over role writes, harness: LiteSVM

## Loaders

- [ ] **INV-XC-18: a ring account loads only as the program's own image**
  - Partial coverage: `program-tests/custom-ring/tests/rules/transact_rules.rs` `a_ring_account_of_another_owner_is_refused` (owner), `program-tests/custom-ring/tests/rules/operator_rules.rs` `a_ring_with_an_unknown_mode_reads_as_invalid` (`valid`), `a_table_write_decodes_and_counts_its_generation_on_the_policy` (a registry in the ring slot), `a_co_signer_write_over_an_occupied_record_needs_that_signer_s_record` (a co-signer image another program owns) (LiteSVM). No test sends a ring account of the right owner with another size or discriminator.
  - Kind: precondition
  - Affects: every ring instruction but Initialize
  - Statement: every `RingPolicy` or `CoSigner` read returns its `INVALID` code unless the account is owned by the invoked program, its length is exactly the struct size, its first byte is the discriminator (15 or 14), and the field rule holds (a known mode, or a nonzero scope and signer).
  - Location: `programs/custom-ring/src/instructions/accounts/loader.rs:38-94` (`fn load`, `fn load_mut`, `struct OwnedAccount`), `program-libs/interface/src/accounts/state.rs:51-62, 112-119` (`fn from_data`, `fn from_data_mut`, `ring_accounts!`)
  - Error: `CustomRingError::InvalidRingPolicy = 8300` / `CustomRingError::InvalidCoSignerRecord = 8260`
  - Severity: Critical (owner and discriminator stand for the canonical address)
  - Suggested test: negative per field, harness: LiteSVM

- [ ] **INV-XC-19: a registry the ring reads loads only as a storage image**
  - Partial coverage: `program-tests/custom-ring/tests/rules/storage_root_rules.rs` `update_policy_rebinds_the_records_to_a_registry_of_any_authority` (the ring account as records) (LiteSVM). No test sends a storage-owned account of another layout at a pinned address.
  - Kind: precondition
  - Affects: Initialize, UpdatePolicy, Transfer, AuthorityTransfer, Merge
  - Statement: every policy registry read returns `InvalidStorageRegistry`, and every tracking registry read returns `InvalidTrackingRegistry`, unless the account is owned by `STORAGE_PROGRAM_ID` with the exact size, discriminator (1 or 3) and a `next_index` in range.
  - Location: `programs/custom-ring/src/instructions/accounts/loader.rs:124-132, 214-222` (`fn load_storage_registry`, `fn load_tracking_registry`), `loader.rs:70-80` (`fn OwnedAccount::load`), `program-libs/interface/src/accounts/storage.rs:75-116` (`storage_account!`)
  - Error: `CustomRingError::InvalidStorageRegistry = 8302` / `CustomRingError::InvalidTrackingRegistry = 8502`
  - Severity: High
  - Suggested test: negative per field, harness: LiteSVM

- [x] **INV-XC-20: every registry the ring reads is the one it pins, compared before its bytes**
  - Covered by: `program-tests/custom-ring/tests/rules/storage_root_rules.rs` `a_registry_the_ring_does_not_bind_is_refused` (stranger, swapped order), `a_forged_tracking_registry_with_a_matching_ring_id_and_an_empty_root_fails_at_load` (LiteSVM), `program-tests/custom-ring/tests/rules/merge_rules.rs` `a_merge_reads_only_the_pinned_tracking_registry` (LiteSVM)
  - Kind: precondition
  - Affects: Transfer, AuthorityTransfer, Merge
  - Statement: the registry window holds exactly the own, curator and tracking registries the root read names, in that order, and every account there whose address differs from the ring's `records`, its one curator or its `tracking` returns Err before any byte of it is read.
  - Location: `programs/custom-ring/src/instructions/accounts/loader.rs:176-210` (`fn RegistrySlots::load`)
  - Error: `CustomRingError::UnboundRegistry = 8480`
  - Severity: Critical (a forged registry would prove any wallet absent)
  - Suggested test: negative per slot, harness: LiteSVM

- [ ] **INV-XC-21: no instruction deserializes an account twice**
  - Not covered.
  - Kind: state
  - Affects: Transfer, AuthorityTransfer, Merge, Initialize, UpdatePolicy
  - Statement: every instruction loads the ring account once, `RegistrySlots::load` loads each registry slot once, and `ProposedSources::check` loads each distinct registry address once.
  - Location: `programs/custom-ring/src/instructions/transfer/processor.rs:33-36`, `programs/custom-ring/src/instructions/accounts/loader.rs:176-210`, `programs/custom-ring/src/instructions/verification/policy_binding.rs:96-116` (`fn ProposedSources::check`, the `checked` list)
  - Severity: Medium
  - Suggested test: review only, harness: none

## Roots and Revisions

- [x] **INV-XC-22: a revision answers only for its current or unexpired root**
  - Covered by: `program-tests/custom-ring/tests/rules/storage_root_rules.rs` `a_policy_transfer_proves_under_a_live_storage_root` (served at slot 999, refused at 1000), `a_curator_revocation_refuses_the_next_subscriber_transfer` (LiteSVM), `sdk-libs/client/tests/accounts.rs` `a_tracking_update_serves_the_old_root_for_its_grace`, `an_allow_list_addition_keeps_grace_and_a_block_list_addition_ends_every_older_root` (library)
  - Kind: state
  - Affects: Transfer, AuthorityTransfer, Merge, storage Update (private writes)
  - Statement: a registry serves revision `r` at slot `s` exactly when some root entry has a nonzero hash, revision `r`, and is the newest entry or has `s` strictly below its expiry. An empty entry never answers for revision 0.
  - Location: `program-libs/interface/src/accounts/storage.rs:289-305` (`fn root_at`)
  - Error: `CustomRingError::RootNotServed = 8481` (ring) / `StorageError::RootExpired = 9003` (storage)
  - Severity: Critical (a retired root proves a revoked list state)
  - Suggested test: boundary at the expiry slot, harness: LiteSVM

- [x] **INV-XC-23: a frame names revisions, never roots**
  - Covered by: `program-tests/custom-ring/tests/rules/storage_root_rules.rs` `a_policy_transfer_proves_under_a_live_storage_root` (LiteSVM), `program-tests/custom-ring/tests/ring/one_key.rs` `one_key_settles_a_real_transfer_and_refuses_three_forgeries` (a forged tracking root proves nothing on chain) (localnet)
  - Kind: state
  - Affects: Transfer, AuthorityTransfer, Merge, storage Update
  - Statement: every root a ring statement or a private apply statement binds is read from a registry account at the named revision, and no instruction data byte reaches a root element.
  - Location: `program-libs/interface/src/instructions/transact.rs:944-957` (`fn roots`), `programs/custom-ring/src/instructions/settlement/merge.rs:63-65`, `programs/storage/src/loader.rs:148-153` (`fn PinnedTrackingRoot::load`)
  - Severity: Critical
  - Suggested test: negative with a real proof under a root the registry does not serve, harness: localnet

- [x] **INV-XC-24: a salted read names no tracking root below its source's floor**
  - Covered by: `program-tests/custom-ring/tests/rules/storage_root_rules.rs` `a_tracking_root_below_the_salted_floor_is_refused_at_load`, `an_unpaired_private_source_cannot_name_an_old_tracking_root`, `a_ring_with_no_salted_list_answers_to_no_floor_of_its_pinned_records`, `a_curated_public_ring_reads_a_curator_pinned_to_its_own_tracking_registry` (LiteSVM)
  - Kind: precondition
  - Affects: Transfer (Own and Policy reads)
  - Statement: every transfer whose tracking revision is strictly below the highest `tracking_floor` among the loaded sources that serve a salted list returns Err, and a source that serves no salted list imposes a floor of 0.
  - Location: `programs/custom-ring/src/instructions/accounts/loader.rs:188-200` (`fn RegistrySlots::load`), `program-libs/interface/src/accounts/state.rs:362-368, 465-472` (`fn tracking_floor`, `fn LiveRoots::tracking`)
  - Error: `CustomRingError::TrackingRootBelowFloor = 8503`
  - Severity: Critical (a stale tracking root proves an enrolled owner absent and skips its salted entry)
  - Suggested test: negative at floor minus 1, positive at the floor, harness: LiteSVM

- [x] **INV-XC-25: a salted source pins the ring's tracking registry**
  - Covered by: `program-tests/custom-ring/tests/rules/salted_source_rules.rs` `initialize_stores_a_salted_source_only_when_it_pairs_with_the_ring`, `a_rule_change_enabling_a_salted_read_pairs_its_source_first`, `an_unpaired_curator_serves_a_public_list_and_no_salted_one`, `a_source_move_pairs_the_salted_source_it_sets`, `a_records_rebind_under_a_salted_list_pairs_the_new_records` (LiteSVM), `program-tests/custom-ring/tests/rules/storage_root_rules.rs` `a_salted_list_reads_its_own_source_under_the_tracking_pin`, `a_private_list_mapped_to_a_curator_on_another_tracking_registry_is_refused` (LiteSVM)
  - Kind: precondition
  - Affects: Initialize, UpdatePolicy, Transfer
  - Statement: every configuration write that would store, and every transfer that reads, a source serving a salted list whose `tracking_registry` differs from the ring's `tracking` returns Err.
  - Location: `program-libs/interface/src/accounts/state.rs:342-355` (`fn check_source_pair`), `programs/custom-ring/src/instructions/verification/policy_binding.rs:96-116` (`fn ProposedSources::check`), `programs/custom-ring/src/instructions/accounts/loader.rs:194` (`fn RegistrySlots::load`)
  - Error: `CustomRingError::SaltedSourceOtherTracking = 8504`
  - Severity: Critical
  - Suggested test: negative per path and pin, harness: LiteSVM

- [x] **INV-XC-26: a revision counts up by one per root move**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `the_program_rotates_roots_as_the_indexer_projects_them` (LiteSVM storage), `program-tests/custom-ring/tests/rules/operator_rules.rs` `pda_enrollment_and_rotation_require_the_vault_manager` (revisions 1 and 2) (LiteSVM)
  - Kind: state
  - Affects: storage Update, ApplyEnrollments
  - Statement: after every successful root move, the newest root's revision is exactly the revision before plus 1, and no two served roots share a revision.
  - Location: `program-libs/interface/src/accounts/storage.rs:255-286` (`fn advance_roots`)
  - Severity: High
  - Suggested test: property over a sequence of applies, harness: LiteSVM storage

## Policy Table Binding

- [x] **INV-XC-27: one write stores the rules, the sources and the hash, after the whole table validates**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `initialize_refuses_each_table_form`, `a_table_write_refuses_each_table_form`, `a_source_write_refuses_each_table_form_it_would_store_again`, `a_rule_over_a_member_written_list_is_refused`, `a_table_write_decodes_and_counts_its_generation_on_the_policy` (LiteSVM), `sdk-libs/client/tests/accounts.rs` `a_policy_write_validates_and_repins_the_table`, `a_policy_write_refuses_each_table_form_and_keeps_the_pinned_table` (library)
  - Kind: state
  - Affects: Initialize, UpdatePolicy (Rules, Source, RingListTag)
  - Statement: every stored table decodes to unique rows the encoder emits with zero padding, holds the named list semantics (no member-written list, policy lists only, one absence per rule, no inverted or mixed polarity), maps every list a rule reads to a source, and `policy_hash` is exactly its hash with the sources and the ring list tag.
  - Location: `program-libs/interface/src/accounts/state.rs:263-277` (`fn set_policy`), `program-libs/policy/src/rule_table/mod.rs:612-655` (`fn decode`, `fn validate`)
  - Error: `CustomRingError::InvalidPolicyRules = 8140` / `MemberWrittenListRule = 8233` / `ListNotPolicyReadable = 8255` / `MultiListAbsenceRule = 8235` / `InvertedListPolarity = 8236` / `MixedPolarityRule = 8237` / `InvalidSource = 8134`
  - Severity: Critical (the circuit holds its table to the hash, a refused table never reaches a proof)
  - Suggested test: negative per form, then the hash pinned, harness: LiteSVM

- [x] **INV-XC-28: the sources name exactly the referenced lists**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `policy_writes_need_a_policy_ring_and_a_foreign_curator` (unreferenced, unsourced), `a_nonzero_source_refuses_the_default_curator_address` (LiteSVM)
  - Kind: precondition
  - Affects: Initialize, UpdatePolicy (Rules)
  - Statement: every table write returns Err when a source spec names an unknown list, a list no rule references, the same list twice, a curator index past the trailing accounts, or a curator at the zero address.
  - Location: `programs/custom-ring/src/instructions/verification/policy_binding.rs:51-67` (`fn TableBinding::sources`), `policy_binding.rs:71-86` (`fn source_registry`)
  - Error: `CustomRingError::InvalidSource = 8134`
  - Severity: High (a source no rule reads would change the hash with no rule behind it)
  - Suggested test: negative per case, harness: LiteSVM

- [ ] **INV-XC-29: a ring reads from at most one curator**
  - Not covered. No test writes sources naming two curators.
  - Kind: state
  - Affects: Initialize, UpdatePolicy (Rules, Source)
  - Statement: every table or source write whose stored sources would name two distinct nonzero registries returns Err.
  - Location: `program-libs/policy/src/rule_table/mod.rs:104-115` (`fn SourceMap::check_curators`), `program-libs/interface/src/accounts/state.rs:690-699` (`fn source_map_of`)
  - Error: `CustomRingError::InvalidSource = 8134`
  - Severity: High (the circuit opens one curator root beside the own one)
  - Suggested test: negative, harness: LiteSVM

- [x] **INV-XC-30: every registry a write needs is among its accounts**
  - Covered by: `program-tests/custom-ring/tests/rules/salted_source_rules.rs` `a_rule_change_enabling_a_salted_read_pairs_its_source_first`, `a_source_move_pairs_the_salted_source_it_sets`, `a_public_source_move_passes_the_salted_source_it_keeps` (LiteSVM)
  - Kind: precondition
  - Affects: Initialize, UpdatePolicy
  - Statement: every configuration write returns Err unless the trailing accounts hold, by address, the registry the write names and every registry a salted list of the proposed account reads.
  - Location: `programs/custom-ring/src/instructions/verification/policy_binding.rs:96-116` (`fn ProposedSources::check`)
  - Error: `CustomRingError::MissingRegistry = 8482`
  - Severity: High
  - Suggested test: negative per missing registry, harness: LiteSVM

## `ring_auth` and CPI

- [ ] **INV-XC-31: the program signs as `ring_auth` only after an address check**
  - Partial coverage: every `MissingRingAuth` test in `program-tests/custom-ring/tests/rules/` (LiteSVM) shows the check. No test inspects that every signed CPI goes through `PlacedRingAuth`.
  - Kind: state
  - Affects: Initialize, Transfer, AuthorityTransfer, Merge, SetPaused, EnrollBatch
  - Statement: every CPI the ring program signs with the `ring_auth` seeds runs through a `PlacedRingAuth` built by `check_account` or `check_slot`, and both refuse an account list without `ring_auth` at the checked position.
  - Location: `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs:104-185` (`struct RingAuth`, `struct PlacedRingAuth`)
  - Error: `CustomRingError::MissingRingAuth = 8104`
  - Severity: Critical (SPP accepts a ring's call on that one signature)
  - Suggested test: review plus negative per instruction, harness: LiteSVM

- [x] **INV-XC-32: `ring_auth` sits only at SPP's `ring_config` slot of a forwarded list**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `the_forwarded_list_holds_the_ring_pda_and_a_signing_payer`, `a_second_ring_pda_in_the_forwarded_list_is_refused` (LiteSVM), `program-tests/custom-ring/tests/rules/merge_rules.rs` `a_merge_forward_holds_the_ring_pda_and_a_signing_payer`, `a_merge_naming_the_ring_pda_past_its_slot_is_refused` (LiteSVM)
  - Kind: precondition
  - Affects: Transfer, AuthorityTransfer, Merge
  - Statement: every forwarded list that lacks `ring_auth` at SPP's `ring_config` index (4 for a transact, 2 for a merge) returns `MissingRingAuth`, and every one that holds it at a second index returns `RingAuthOutOfSlot`.
  - Location: `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs:128-144` (`fn RingAuth::check_slot`)
  - Error: `CustomRingError::MissingRingAuth = 8104` / `CustomRingError::RingAuthOutOfSlot = 8234`
  - Severity: Critical (SPP reads every copy of the PDA as signed, a second copy would pass as an owner or a funder)
  - Suggested test: negative per position, harness: LiteSVM

- [ ] **INV-XC-33: a signed CPI raises `ring_auth` alone**
  - Partial coverage: `program-tests/custom-ring/tests/rules/operator_rules.rs` `pda_enrollment_requires_one_owner_signature_per_invocation` (an unsigned owner stays unsigned through `EnrollBatch`) (LiteSVM). No test inspects the metas of a transfer CPI.
  - Kind: postcondition
  - Affects: Initialize, Transfer, AuthorityTransfer, Merge, SetPaused, EnrollBatch
  - Statement: in every ring program CPI, each account meta keeps the writable flag and the signer flag it held on entry, except that every account at the `ring_auth` address is a signer.
  - Location: `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs:158-184` (`fn PlacedRingAuth::invoke_program`)
  - Severity: Critical
  - Suggested test: positive with inner instruction meta inspection, harness: LiteSVM

- [ ] **INV-XC-34: a CPI holds at most 128 accounts**
  - Not covered. `program-tests/custom-ring/tests/rules/merge_rules.rs` `a_forward_past_the_cpi_account_cap_is_refused` reaches the ring proof check first.
  - Kind: precondition
  - Affects: Transfer, AuthorityTransfer, Merge
  - Statement: every forwarded list longer than `MAX_CPI_ACCOUNTS` (128) returns Err before the CPI.
  - Location: `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs:17, 164-166` (`fn invoke_program`)
  - Error: `CustomRingError::TooManyAccounts = 8115`
  - Severity: Medium
  - Suggested test: negative with a passing ring proof, harness: localnet

- [ ] **INV-XC-35: each CPI goes to one fixed program**
  - Partial coverage: success tests reach SPP and storage. No test inspects every inner instruction's program id.
  - Kind: state
  - Affects: Initialize, Transfer, AuthorityTransfer, Merge, SetPaused, EnrollBatch, SetCoSigner, storage Initialize, InitializeTracking, OwnerGrant, ApplyEnrollments
  - Statement: the ring program calls SPP for `Transfer`, `AuthorityTransfer`, `Merge`, `SetPaused` and the end of `Initialize`, the storage program for `Initialize` and `EnrollBatch`, and the system program only to create its PDAs. The storage program calls the system program only to create its PDAs and itself only for the enrollment event. No path calls a program id taken from an account or from data.
  - Location: `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs:148-154` (`fn invoke`), `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:145-162`, `programs/custom-ring/src/instructions/tracking/enroll_batch.rs:43-52`, `programs/storage/src/lib.rs:69-80` (`fn emit_event`)
  - Severity: Critical
  - Suggested test: positive with inner instruction inspection, harness: LiteSVM

- [x] **INV-XC-36: every SPP call names the executable SPP program**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `a_transfer_without_the_shielded_pool_program_refuses_before_verification` (LiteSVM), `program-tests/custom-ring/tests/rules/merge_rules.rs` `a_merge_without_the_shielded_pool_program_refuses_before_verification` (LiteSVM), `program-tests/custom-ring/tests/rules/operator_rules.rs` `only_the_pauser_pauses_through_the_ring_pda_and_the_pool` (LiteSVM)
  - Kind: precondition
  - Affects: Initialize, Transfer, AuthorityTransfer, Merge, SetPaused
  - Statement: every instruction that calls SPP returns Err when no account of its SPP list is the executable SPP program id.
  - Location: `programs/custom-ring/src/instructions/accounts/loader.rs:226-235` (`fn validate_spp_program`)
  - Error: `CustomRingError::InvalidShieldedPoolProgram = 8103`
  - Severity: Medium
  - Suggested test: negative per instruction, harness: LiteSVM

- [x] **INV-XC-37: the payer SPP debits signs**
  - Covered by: `program-tests/custom-ring/tests/rules/transact_rules.rs` `the_forwarded_list_holds_the_ring_pda_and_a_signing_payer`, `the_program_reads_each_signer_where_the_layout_names_it` (LiteSVM), `program-tests/custom-ring/tests/rules/merge_rules.rs` `a_merge_forward_holds_the_ring_pda_and_a_signing_payer` (LiteSVM)
  - Kind: precondition
  - Affects: Transfer, AuthorityTransfer, Merge
  - Statement: every forwarded list whose payer (index 0 of a transact, index 3 of a merge) is unsigned returns Err.
  - Location: `programs/custom-ring/src/instructions/transfer/accounts.rs:168`, `programs/custom-ring/src/instructions/settlement/merge.rs:57`, `programs/custom-ring/src/instructions/accounts/loader.rs:237-248` (`fn check_funding_signer`)
  - Error: `CustomRingError::MissingFundingSigner = 8173`
  - Severity: High
  - Suggested test: negative per instruction, harness: LiteSVM

- [ ] **INV-XC-38: the ring relies on SPP for the state it settles**
  - Not testable here. INSUFFICIENT_INFO: note membership, nullifier uniqueness, value conservation, expiry, the activation and pause of `RingConfig`, the authority rail flag and the validation of every settlement account live in SPP's ring rails and circuits under `zolana/`, outside the analyzed source.
  - Kind: state
  - Affects: Transfer, AuthorityTransfer, Merge
  - Statement: the ring program checks none of these, and settles only what SPP's proof and checks accept under the `ring_auth` signature.
  - Location: `programs/custom-ring/src/instructions/transfer/processor.rs:11-12` (module contract), `programs/custom-ring/src/lib.rs:1-32`
  - Severity: Critical
  - Suggested test: none in this crate, harness: SPP's own suites

## Proof System

- [x] **INV-XC-39: every ring proof has one public input and a mandatory commitment**
  - Covered by: `program-libs/interface/src/proof/verification.rs` `each_proof_stage_fails_with_its_own_code` (unit), `program-tests/custom-ring/tests/ring/one_key.rs` `one_key_settles_a_real_transfer_and_refuses_three_forgeries` (one flipped statement bit fails) (localnet)
  - Kind: precondition
  - Affects: Transfer, AuthorityTransfer, Merge, storage Update, ApplyEnrollments
  - Statement: every ring proof verifies only under a BSB22 key with the statement hash as its single public input. A point that does not decompress returns `InvalidRingProofPoint`, a key without a commitment returns `UnsupportedCircuit`, and a failed pairing returns `ProofVerificationFailed`. The storage program maps all three to its own `ProofVerificationFailed`.
  - Location: `program-libs/interface/src/proof/verification.rs:28-43` (`fn RingProof::verify`), `programs/storage/src/instructions/update.rs:95-101`, `programs/storage/src/instructions/apply_enrollments.rs:101-109`
  - Error: `CustomRingError::InvalidRingProofPoint = 8238` / `UnsupportedCircuit = 8113` / `ProofVerificationFailed = 8101` / `StorageError::ProofVerificationFailed = 9008`
  - Severity: Critical
  - Suggested test: negative per stage, harness: unit

- [x] **INV-XC-40: every statement follows its circuit's element order**
  - Covered by: `program-libs/interface/tests/statements.rs` `ring_statements_vector`, `the_anonymous_transfer_statement_follows_the_contract_order`, `the_element_count_follows_the_family_alone`, `the_record_chain_tags_each_published_slot_with_its_exit_claim`, `a_flipped_exit_claim_at_equal_length_moves_the_record_chain`, `the_ciphertext_digest_tags_the_length_over_the_left_packed_chain` (library)
  - Kind: state
  - Affects: Transfer, AuthorityTransfer, Merge, storage Update, ApplyEnrollments
  - Statement: the transfer, merge, apply records, apply enrollments and register key statements hash their elements in exactly the order of their Go circuits, pinned by `test-vectors/proof/ring-statements.json`.
  - Location: `program-libs/interface/src/proof/statement.rs:140-188, 388-488` (`fn TransferStatement::elements`, `fn ApplyRecordsStatement::elements`, `fn ApplyEnrollmentsStatement::elements`, `fn RegisterKeyStatement::elements`), `program-libs/interface/src/instructions/merge.rs:110-133` (`fn MergePublicInput::elements`)
  - Severity: Critical
  - Suggested test: golden vectors, harness: library

- [ ] **INV-XC-41: each statement and each write chain opens under its own domain**
  - Partial coverage: the vectors of INV-XC-40 pin the domain fields. No test shows a proof of one kind refused under another kind's statement.
  - Kind: state
  - Affects: Transfer, AuthorityTransfer, Merge, storage Update, ApplyEnrollments
  - Statement: the first element of every statement is exactly its own domain field (`TRANSFER_STATEMENT`, `MERGE_STATEMENT`, `APPLY_RECORDS`, `APPLY_ENROLLMENTS`, `REGISTER_KEY`), and the records chain and the enrollment chain start under `APPLY_RECORDS` and `APPLY_ENROLLMENTS`.
  - Location: `program-libs/interface/src/proof/statement.rs:72-90` (`fn AuditHead::elements_under`), `program-libs/interface/src/accounts/storage.rs:219-236` (`fn chain_start`, `fn chain_start_under`), `program-libs/interface/src/accounts/tracking.rs:262-267` (`fn tracking_chain_start`)
  - Severity: High
  - Suggested test: unit over the domain table, harness: library

- [x] **INV-XC-42: every catalog circuit has a compiled verifying key**
  - Covered by: `sdk-libs/client/tests/vk_proving_key_lock.rs` `key_catalog_matches_circuits` (every circuit the program selects for a supported shape has a catalog key, and each key's verifying key exists exactly when its proving key is pinned), `every_catalog_key_is_locked_with_its_verifying_key` (library)
  - Kind: state
  - Affects: Transfer, AuthorityTransfer, Merge, storage Update, ApplyEnrollments
  - Statement: `verifying_key` returns a key for every transfer shape of `TRANSFER_SHAPES`, for every apply width 1, 2, 4, 8, 16 in both modes, for every enrollment width 1, 2, 4, 8, for `RegisterKey` and for `Merge`.
  - Location: `program-libs/interface/src/verifying_keys/mod.rs:161-300` (`fn verifying_key`), `program-libs/interface/src/proof/key_catalog.rs:153-188` (`fn RingCircuit::transfer`, `fn apply_records`, `fn apply_enrollments`, `fn apply_width`)
  - Severity: High
  - Suggested test: unit over the catalog, harness: library

## Lamports and PDAs

- [ ] **INV-XC-43: every PDA is created at its canonical bump**
  - Partial coverage: `program-tests/custom-ring/tests/rules/operator_rules.rs` `initialize_takes_its_canonical_ring_address_once_beside_the_system_program`, `initialize_refuses_a_tracking_account_off_its_seeds` (LiteSVM), `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_tracking_registry_pins_ring_and_auditor_at_its_canonical_address` (LiteSVM storage). No negative test for a co-signer record, a policy registry or an owner grant at a non-canonical address of the same seeds.
  - Kind: precondition
  - Affects: Initialize, SetCoSigner, storage Initialize, InitializeTracking, OwnerGrant
  - Statement: every created account's address is the PDA `derive_program_address` returns for its seeds, its bump comes from that derivation, and no instruction reads a bump from data.
  - Location: `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs:55-83` (`fn Pda::create`), `programs/storage/src/loader.rs:336-368` (`fn CanonicalPda::bump`, `fn create`)
  - Error: the ring account type's `INVALID` code or the storage loader's `invalid` code (`InvalidRingPolicy`, `InvalidCoSignerRecord`, `InvalidRegistry`, `InvalidTrackingRegistry`, `InvalidOwnerGrant`)
  - Severity: Critical
  - Suggested test: negative per creation, harness: LiteSVM

- [ ] **INV-XC-44: a donated balance cannot block a creation**
  - Not covered.
  - Kind: reachability
  - Affects: Initialize, SetCoSigner, storage Initialize, InitializeTracking, OwnerGrant
  - Statement: for every PDA a program creates, a balance sent to the address before creation does not make the creation fail.
  - Location: `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs:74-81` (`fn Pda::create`), `programs/storage/src/loader.rs:360-367` (`fn CanonicalPda::create`)
  - Severity: High (a blocked ring or tracking PDA would be permanent)
  - Suggested test: positive after a donation, harness: LiteSVM

- [ ] **INV-XC-45: lamports move only on creation and on the co-signer close**
  - Partial coverage: no test sums lamports across an instruction.
  - Kind: state
  - Affects: all 12 ring and all 7 storage instructions
  - Statement: the programs move lamports only to fund a created PDA from the payer and to close a co-signer record into its rent recipient. Every other change of balance in a ring transaction comes from SPP or the runtime fee.
  - Location: `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs:55-102` (`fn Pda::create`, `fn close_into`), `programs/storage/src/loader.rs:349-368` (`fn CanonicalPda::create`)
  - Severity: High
  - Suggested test: property over the lamport sum per instruction, harness: LiteSVM

- [ ] **INV-XC-46: a fee payer appears only where an account is created**
  - Partial coverage: `sdk-libs/client/tests/instruction_builders.rs` pins the builders' account lists (library). No on-chain check.
  - Kind: state
  - Affects: all instructions
  - Statement: the ring's `Initialize` and `SetCoSigner` and the storage `Initialize`, `InitializeTracking` and `OwnerGrant` take a payer, `Transfer`, `AuthorityTransfer` and `Merge` forward SPP's payer, and no other instruction takes a payer account.
  - Location: `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:49`, `programs/custom-ring/src/instructions/configuration/access/set_cosigner.rs:32`, `programs/storage/src/instructions/initialize.rs:25`, `programs/storage/src/instructions/initialize_tracking.rs:28`, `programs/storage/src/instructions/owner_grant.rs:27`
  - Severity: Medium
  - Suggested test: review plus builder pins, harness: library

## Build

- [x] **INV-XC-47: the deployed binaries keep every data pointer**
  - Covered by: `program-tests/custom-ring/tests/rules/binary_rules.rs` `every_data_pointer_keeps_its_addend_through_the_loader` (both binaries)
  - Kind: state
  - Affects: every instruction of both programs
  - Statement: neither `custom_ring_program.so` nor `storage_program.so` holds an `R_X86_64_RELATIVE` relocation outside `.text` whose low word is nonzero.
  - Location: `programs/custom-ring`, `programs/storage` (the release build)
  - Severity: High (the loader reads only the high word, a lost addend reads another constant)
  - Suggested test: exists, harness: LiteSVM

## Statement Hashes

- [ ] **INV-XC-48: a new root off the field fails the apply statement hash**
  - Not covered. No test sends a `new_root` at or above the BN254 scalar modulus.
  - Kind: precondition
  - Affects: storage Update, ApplyEnrollments
  - Statement: every `Update` and every `ApplyEnrollments` whose `new_root` is at or above the BN254 scalar modulus passes the parse and returns Err at the statement hash, before the proof check. No parse checks `new_root` against the field. `ApplyEnrollments` writes the root ahead of the hash, and the refusal reverts it (`INV-XC-09`).
  - Location: `programs/storage/src/instructions/update.rs:95-97` (`fn process_update_ix`), `programs/storage/src/instructions/apply_enrollments.rs:85-90, 101-103` (`fn process_apply_enrollments_ix`), `program-libs/interface/src/proof/statement.rs:413-415, 446-448` (`fn ApplyRecordsStatement::hash`, `fn ApplyEnrollmentsStatement::hash`)
  - Error: `StorageError::HashingFailed = 9009`
  - Severity: Medium (no root off the field lands)
  - Suggested test: negative at the modulus for each instruction, harness: LiteSVM storage
