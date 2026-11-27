# Storage Invariants

Covers the storage program's `Initialize` (tag 0), `Update` (tag 1), `UpdateAuthority` (tag 4), `OwnerGrant` (tag 8) and `EmitEvent` (tag 10). `InitializeTracking` and `ApplyEnrollments` live in `enrollment.md`.

A policy registry keeps a root history over its records tree, and the authority moves it only with an apply proof. The shared root history rule, the canonical PDA creation, the dispatch and the error codes live in `cross-cutting.md`.

## Storage Initialize

### Accounts and Authorization

- [ ] **INV-ST-INIT-01: the instruction names four accounts**
  - Not covered.
  - Kind: precondition
  - Statement: every `Initialize` with fewer than four accounts returns Err.
  - Location: `programs/storage/src/instructions/initialize.rs:25-28` (`fn process_initialize_ix`)
  - Error: `StorageError::MissingAccounts = 9010`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM storage

- [ ] **INV-ST-INIT-02: the payer and the creator sign**
  - Not covered.
  - Kind: precondition
  - Statement: every `Initialize` whose payer or creator is not a signer returns Err.
  - Location: `programs/storage/src/instructions/initialize.rs:29-30` (`fn process_initialize_ix`)
  - Error: `StorageError::MissingSignature = 9011`
  - Severity: High (the creator becomes the authority)
  - Suggested test: negative per account, harness: LiteSVM storage

- [ ] **INV-ST-INIT-03: the fourth account is the system program**
  - Not covered.
  - Kind: precondition
  - Statement: every `Initialize` whose fourth account is not the system program id returns Err.
  - Location: `programs/storage/src/instructions/initialize.rs:31-33` (`fn process_initialize_ix`)
  - Error: `StorageError::InvalidSystemProgram = 9013`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM storage

### Instruction Data and Create

- [ ] **INV-ST-INIT-04: the data is exactly 104 bytes**
  - Not covered.
  - Kind: precondition
  - Statement: every `Initialize` whose data after the tag is not exactly `InitializeIxData::SIZE` (104) bytes returns Err.
  - Location: `programs/storage/src/instructions/initialize.rs:24` (`fn process_initialize_ix`), `programs/storage/src/lib.rs:82-84` (`fn parse`)
  - Error: `StorageError::InvalidInstructionData = 9005`
  - Severity: Medium
  - Suggested test: negative at 103 and 105, harness: LiteSVM storage

- [x] **INV-ST-INIT-05: the registry sits at the creator's canonical PDA**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_registry_takes_its_canonical_address_and_loads_owned_writable_and_initialized` (the PDA of another registry id, a read-only account) (LiteSVM storage)
  - Kind: precondition
  - Statement: every `Initialize` whose registry account is not the writable PDA of `[b"storage", creator, registry_id]` returns Err.
  - Location: `programs/storage/src/loader.rs:40-47` (`fn RegistryInit::init`), `loader.rs:338-345` (`fn CanonicalPda::bump`)
  - Error: `StorageError::InvalidRegistry = 9000`
  - Severity: High (no other signer can take a creator's address)
  - Suggested test: negative, harness: LiteSVM storage

- [x] **INV-ST-INIT-06: a registry is created once**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_curator_initializes_without_a_ring` (LiteSVM storage)
  - Kind: precondition
  - Statement: every `Initialize` over an account that holds data returns Err.
  - Location: `programs/storage/src/loader.rs:351-353` (`fn CanonicalPda::create`)
  - Error: `StorageError::AlreadyInitialized = 9014`
  - Severity: High
  - Suggested test: negative, harness: LiteSVM storage

- [ ] **INV-ST-INIT-07: a new registry holds the sentinel root at revision zero**
  - Partial coverage: `program-tests/custom-ring/tests/storage_lifecycle.rs` `the_program_rotates_roots_as_the_indexer_projects_them` and `a_registry_takes_its_canonical_address_and_loads_owned_writable_and_initialized` compare the program's registry with `StorageRegistry::new`, which runs the same `init` (LiteSVM storage). No test asserts each field after `Initialize` on its own.
  - Kind: postcondition
  - Statement: after a successful `Initialize`, the registry is exactly `StorageRegistry::SIZE` (1657) bytes with discriminator 1, `authority` equal to the creator, the requested `grace_slots`, `ring_id` and `tracking_registry`, `next_index` 1, `tracking_floor` 0, `roots[0]` equal to `MEMBER_REGISTRY_EMPTY_ROOT` at revision 0, and every other root entry zero.
  - Location: `programs/storage/src/loader.rs:48-50` (`fn RegistryInit::init`), `program-libs/interface/src/accounts/storage.rs:150-159` (`fn StorageRegistry::init`)
  - Severity: High
  - Suggested test: positive with a field compare, harness: LiteSVM storage

- [ ] **INV-ST-INIT-08: the grace, the ring id and the tracking pin are final**
  - Not covered as a property.
  - Kind: state
  - Statement: after `Initialize`, no storage instruction changes the registry's `grace_slots`, `ring_id` or `tracking_registry`.
  - Location: `program-libs/interface/src/accounts/storage.rs:183-215` (`fn StorageRegistry::update`), `programs/storage/src/instructions/update_authority.rs:27` (`fn process_update_authority_ix`, the only other writer)
  - Severity: Critical (the ring reads `ring_id` for its own records and `tracking_registry` for the salted pairing)
  - Suggested test: property over every storage instruction, harness: LiteSVM storage

- [ ] **INV-ST-INIT-09: storage checks no ring field at creation**
  - Not covered.
  - Kind: state
  - Statement: `Initialize` accepts every `ring_id`, every `tracking_registry` and every `grace_slots`, and the ring program alone decides whether it reads the registry.
  - Location: `programs/storage/src/instructions/initialize.rs:1-9` (module contract), `program-libs/interface/src/accounts/state.rs:342-355` (`fn RingPolicy::check_source_pair`), `programs/custom-ring/src/instructions/accounts/loader.rs:135-145` (`fn load_records_registry`)
  - Severity: Medium
  - Suggested test: positive with a foreign ring id, then a ring refusal, harness: LiteSVM

- [ ] **INV-ST-INIT-10: a creation changes the payer and the new registry alone**
  - Not covered.
  - Kind: frame
  - Statement: after every successful `Initialize`, every account other than the payer and the registry is unchanged, and the payer's lamports decrease by exactly the rent the registry takes beyond a donated balance.
  - Location: `programs/storage/src/loader.rs:349-368` (`fn CanonicalPda::create`)
  - Severity: Medium
  - Suggested test: positive with lamport deltas, harness: LiteSVM storage

## Storage Update

### Instruction Data Validation

- [ ] **INV-ST-UPDATE-01: an update holds 1 to 16 writes**
  - Partial coverage: `sdk-libs/client/tests/accounts.rs` `update_data_round_trips_and_refuses_no_writes` (library) covers 0 writes. No test sends 17 writes, and no on-chain test sends either count.
  - Kind: precondition
  - Statement: every `Update` whose write count is 0 returns `InvalidInstructionData`, and every one whose count is above `APPLY_BATCH` (16) returns `BatchTooLarge`.
  - Location: `program-libs/interface/src/instructions/storage.rs:440` (`fn ParsedStorageUpdate::parse`), `storage.rs:514-519` (`fn storage_write_count`)
  - Error: `StorageError::InvalidInstructionData = 9005` / `StorageError::BatchTooLarge = 9006`
  - Severity: Medium
  - Suggested test: negative at 0 and 17, harness: LiteSVM storage

- [ ] **INV-ST-UPDATE-02: every write stays inside its mask and the authority's lists**
  - Partial coverage: `sdk-libs/client/tests/accounts.rs` `a_mint_settings_tag_above_one_is_no_entry` (library) and `program-libs/interface/src/instructions/instruction.rs` `the_authority_writes_only_the_authority_lists`, `a_member_writes_only_its_own_lists_within_the_mask` (unit) assert the library refusals under their `CustomRingError` names. No test asserts the storage program's `InvalidInstructionData` for them.
  - Kind: precondition
  - Statement: every `Update` with a write whose member lies outside 1 to `FIELD_MAX - 1`, whose value sets a bit outside its mask, whose mask is zero without new mint settings, whose mask names a list outside `ListSet::STORAGE_WRITES`, whose mint settings tag is above 1, or whose asset limits fail their check, returns Err.
  - Location: `program-libs/interface/src/instructions/storage.rs:486-496` (`fn RecordWriteData::storage_entry`), `program-libs/interface/src/instructions/instruction.rs:243-308` (`fn entry_within`, `fn list_and_mint_entry`), `program-libs/interface/src/instructions/storage.rs:404` (`fn classify`)
  - Error: `StorageError::InvalidInstructionData = 9005`
  - Severity: High (the apply circuit takes the chained value as given)
  - Suggested test: negative per rule on chain, harness: LiteSVM storage

- [ ] **INV-ST-UPDATE-03: asset preimages exempt only their own private writes**
  - Partial coverage: `sdk-libs/client/tests/accounts.rs` `asset_preimages_exempt_only_the_matching_private_write`, `asset_write_indices_are_distinct_and_in_increasing_order` (library). No on-chain test.
  - Kind: precondition
  - Statement: every `Update` returns Err when it names more asset writes than writes, when an asset write's index is not strictly above the previous one, names a write that is not private, or names a mint whose `Member::asset` differs from that write's member, or when an asset write is left past the last write.
  - Location: `program-libs/interface/src/instructions/storage.rs:395-432` (`fn RecordWriteClassification::classify`), `storage.rs:443-446` (`fn ParsedStorageUpdate::parse`)
  - Error: `StorageError::InvalidAssetWrite = 9034`
  - Severity: High (an exempt write proves no tracking row)
  - Suggested test: negative per case, harness: LiteSVM storage

- [x] **INV-ST-UPDATE-04: a tracking revision follows exactly a private owner write**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_tracking_revision_follows_exactly_a_private_owner_write` (LiteSVM storage), `sdk-libs/client/tests/accounts.rs` `the_parser_reads_a_tracking_revision_exactly_when_a_write_is_private`, `a_public_update_carries_no_tracking_revision` (library)
  - Kind: precondition
  - Statement: the data holds a tracking revision exactly when the private write bitmap the masks derive is nonzero. Every other layout fails the exact length of the apply data.
  - Location: `program-libs/interface/src/instructions/storage.rs:454-459` (`fn ParsedStorageUpdate::parse`)
  - Error: `StorageError::InvalidInstructionData = 9005`
  - Severity: High (no flag chooses the apply key)
  - Suggested test: negative per private list, harness: LiteSVM storage

- [x] **INV-ST-UPDATE-05: the restrictive byte is 0 or 1**
  - Covered by: `sdk-libs/client/tests/accounts.rs` `the_restrictive_byte_has_one_encoding` (library)
  - Kind: precondition
  - Statement: every `Update` whose restrictive byte is above 1 returns Err.
  - Location: `program-libs/interface/src/instructions/storage.rs:461-463` (`fn ParsedStorageUpdate::parse`)
  - Error: `StorageError::InvalidInstructionData = 9005`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM storage

### Accounts and Authorization

- [ ] **INV-ST-UPDATE-06: the instruction names the authority and the registry**
  - Not covered.
  - Kind: precondition
  - Statement: every `Update` with fewer than two accounts returns Err.
  - Location: `programs/storage/src/instructions/update.rs:38-40` (`fn process_update_ix`)
  - Error: `StorageError::MissingAccounts = 9010`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM storage

- [ ] **INV-ST-UPDATE-07: the authority signs**
  - Not covered. Every test signs the first account.
  - Kind: precondition
  - Statement: every `Update` whose first account is not a signer returns Err.
  - Location: `programs/storage/src/instructions/update.rs:42` (`fn process_update_ix`)
  - Error: `StorageError::MissingSignature = 9011`
  - Severity: Critical
  - Suggested test: negative, harness: LiteSVM storage

- [x] **INV-ST-UPDATE-08: the registry is a writable program policy registry**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_registry_takes_its_canonical_address_and_loads_owned_writable_and_initialized` (a foreign account writable and read-only, a tracking registry writable and read-only, the policy registry read-only) (LiteSVM storage)
  - Kind: precondition
  - Statement: every `Update` whose second account is not program-owned returns `InvalidRegistry`, is read-only returns `ReadOnlyRegistry`, or is not a `StorageRegistry` of exact size, discriminator 1 and `next_index` in 1 to `QUAD_CAPACITY` returns `InvalidRegistry`.
  - Location: `programs/storage/src/loader.rs:169-183` (`fn load_registry_mut`), `loader.rs:206-220` (`fn ProgramAccount::load_mut`), `program-libs/interface/src/accounts/storage.rs:114-116` (`storage_account!` validity)
  - Error: `StorageError::InvalidRegistry = 9000` / `StorageError::ReadOnlyRegistry = 9012`
  - Severity: High
  - Suggested test: negative per field, with a tracking registry in the slot, harness: LiteSVM storage

- [x] **INV-ST-UPDATE-09: only the registry's authority updates**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `an_update_reaches_the_proof_past_its_revision_and_authority`, `authority_rotates_and_the_old_authority_is_refused` (LiteSVM storage)
  - Kind: precondition
  - Statement: every `Update` whose first account is not the registry's `authority` returns Err.
  - Location: `programs/storage/src/instructions/update.rs:45` (`fn process_update_ix`), `programs/storage/src/loader.rs:224-229` (`fn check_authority`)
  - Error: `StorageError::Unauthorized = 9001`
  - Severity: Critical (every policy record of the registry moves under this key)
  - Suggested test: negative, harness: LiteSVM storage

- [x] **INV-ST-UPDATE-10: the update names the registry's current revision**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `an_update_reaches_the_proof_past_its_revision_and_authority`, `a_forged_root_fails_the_real_proof` (a replayed update), `private_proof_failure_and_stale_revision_keep_the_root_and_floor` (LiteSVM storage)
  - Kind: precondition
  - Statement: every `Update` whose revision differs from the newest root's revision returns Err before the proof check.
  - Location: `programs/storage/src/instructions/update.rs:46-48` (`fn process_update_ix`)
  - Error: `StorageError::RevisionChanged = 9002`
  - Severity: High (a replay of a landed update)
  - Suggested test: negative, harness: LiteSVM storage

### Private Writes

- [x] **INV-ST-UPDATE-11: a private write needs a registry pinned to a tracking registry**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_private_write_requires_a_registry_pinned_to_a_tracking_registry` (LiteSVM storage), `sdk-libs/client/tests/accounts.rs` `private_writes_require_a_tracking_pin_before_any_root_changes` (library)
  - Kind: precondition
  - Statement: every `Update` with a private write on a registry whose `tracking_registry` is zero returns Err and leaves the registry unchanged.
  - Location: `programs/storage/src/instructions/update.rs:58-61` (`fn process_update_ix`)
  - Error: `StorageError::PrivateWriteWithoutTracking = 9035`
  - Severity: High
  - Suggested test: negative, harness: LiteSVM storage

- [x] **INV-ST-UPDATE-12: a private write passes exactly the pinned tracking registry**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_private_write_refuses_a_missing_or_substituted_tracking_registry` (absent, missing, substituted, extra) (LiteSVM storage)
  - Kind: precondition
  - Statement: every `Update` with a private write returns Err unless exactly one account follows the registry, its address is the registry's `tracking_registry`, and it is a program-owned `TrackingRegistry`.
  - Location: `programs/storage/src/instructions/update.rs:52-74` (`fn process_update_ix`), `programs/storage/src/loader.rs:148-153` (`fn PinnedTrackingRoot::load`), `loader.rs:125-135` (`fn load_tracking_registry`), `loader.rs:193-203` (`fn ProgramAccount::load`)
  - Error: `StorageError::InvalidTrackingRegistry = 9022`
  - Severity: Critical (a row proven under another registry guards no salted read)
  - Suggested test: negative per case, harness: LiteSVM storage

- [x] **INV-ST-UPDATE-13: a public update takes no tracking account**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `public_writes_refuse_an_extra_tracking_account` (LiteSVM storage)
  - Kind: precondition
  - Statement: every `Update` without a private write that passes an account after the registry returns Err.
  - Location: `programs/storage/src/instructions/update.rs:52-57, 73` (`fn process_update_ix`)
  - Error: `StorageError::InvalidTrackingRegistry = 9022`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM storage

- [x] **INV-ST-UPDATE-14: a private write names a tracking root the registry serves**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_private_write_names_a_tracking_root_the_registry_serves` (LiteSVM storage)
  - Kind: precondition
  - Statement: every `Update` whose tracking revision names no root the pinned tracking registry serves at the current slot returns Err.
  - Location: `programs/storage/src/loader.rs:152` (`fn PinnedTrackingRoot::load`), `program-libs/interface/src/accounts/storage.rs:289-305` (`fn root_at`)
  - Error: `StorageError::RootExpired = 9003`
  - Severity: High
  - Suggested test: negative, harness: LiteSVM storage

### Apply Proof

- [x] **INV-ST-UPDATE-15: the apply proof binds the transition, the ordered writes and the restrictive bit**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `record_writes_apply_with_a_real_proof`, `a_block_list_addition_claimed_permissive_fails_the_real_proof`, `a_forged_root_fails_the_real_proof` (LiteSVM storage)
  - Kind: precondition
  - Statement: every `Update` returns Err unless its proof verifies over `[APPLY_RECORDS, old root, new root, old next index, new next index, chain start, chain end, count, restrictive]`, with the old values read from the account, the chain started at `(registry, revision)` and linked over every write in instruction order, unchanged leaves included.
  - Location: `programs/storage/src/instructions/update.rs:76-101` (`fn process_update_ix`), `program-libs/interface/src/accounts/storage.rs:219-236` (`fn chain_start`), `program-libs/interface/src/proof/statement.rs:388-411` (`fn ApplyRecordsStatement::elements`)
  - Error: `StorageError::ProofVerificationFailed = 9008`
  - Severity: Critical (a restrictive batch labelled permissive would keep a revoked root alive)
  - Suggested test: negative per element with a real proof, harness: LiteSVM storage

- [x] **INV-ST-UPDATE-16: the private bitmap picks the private key and enters the statement**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `each_mode_reaches_its_own_apply_key` (LiteSVM storage)
  - Kind: precondition
  - Statement: an `Update` with a private write verifies only under the private apply key of its width over the statement extended by the private bitmap and the served tracking root, and an `Update` without one only under the public key.
  - Location: `program-libs/interface/src/proof/statement.rs:377-416` (`fn ApplyRecordsStatement::mode`, `fn circuit`, `fn elements`), `program-libs/interface/src/proof/key_catalog.rs:161-166` (`fn RingCircuit::apply_records`)
  - Error: `StorageError::ProofVerificationFailed = 9008`
  - Severity: Critical
  - Suggested test: negative across keys with real proofs, harness: LiteSVM storage

### Success Postconditions

- [x] **INV-ST-UPDATE-17: a restrictive update retires every earlier root at once**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `record_writes_apply_with_a_real_proof` (the root before a block list write stops being served) (LiteSVM storage), `sdk-libs/client/tests/accounts.rs` `an_allow_list_addition_keeps_grace_and_a_block_list_addition_ends_every_older_root` (library)
  - Kind: postcondition
  - Statement: after a successful `Update` whose restrictive bit is 1, the registry serves exactly one root, the new one at revision `r + 1`.
  - Location: `program-libs/interface/src/accounts/storage.rs:195-202, 278` (`fn StorageRegistry::update`, `fn advance_roots`, `Retirement::Immediate`)
  - Severity: Critical (a revocation binds in the next transaction)
  - Suggested test: positive then a read of every older revision, harness: LiteSVM storage

- [x] **INV-ST-UPDATE-18: a permissive update keeps the outgoing root for the grace**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `record_writes_apply_with_a_real_proof` (an approval grant keeps the previous root), `the_program_rotates_roots_as_the_indexer_projects_them` (LiteSVM storage), `sdk-libs/client/tests/accounts.rs` `a_permissive_update_over_a_full_history_evicts_the_oldest_root` (library)
  - Kind: postcondition
  - Statement: after a successful `Update` at slot `s` whose restrictive bit is 0, the new root sits at `roots[0]` at revision `r + 1`, the outgoing root sits at `roots[1]` with expiry `s + grace_slots` (saturating), and a full history drops its oldest entry.
  - Location: `program-libs/interface/src/accounts/storage.rs:195-202, 272-277` (`fn StorageRegistry::update`, `fn advance_roots`, `Retirement::Graced`)
  - Severity: High
  - Suggested test: positive with expiries read back, harness: LiteSVM storage

- [x] **INV-ST-UPDATE-19: the next index stays within the records tree**
  - Covered by: `program-libs/interface/src/accounts/storage.rs` `update_refuses_next_index_outside_quad_capacity` (unit)
  - Kind: precondition
  - Statement: every `Update` whose new next index is 0 or strictly greater than `QUAD_CAPACITY` returns Err after the proof check and leaves the registry unchanged.
  - Location: `program-libs/interface/src/accounts/storage.rs:185-187` (`fn StorageRegistry::update`)
  - Error: `StorageError::InvalidInstructionData = 9005`
  - Severity: Medium
  - Suggested test: negative, harness: unit

- [ ] **INV-ST-UPDATE-20: a private update raises the tracking floor and the floor never falls**
  - Partial coverage: `sdk-libs/client/tests/accounts.rs` `private_updates_keep_the_highest_tracking_floor_and_retire_old_roots` (library) and `program-tests/custom-ring/tests/storage_lifecycle.rs` `private_proof_failure_and_stale_revision_keep_the_root_and_floor` (the refused legs) (LiteSVM storage). No on-chain test lands a private update.
  - Kind: postcondition
  - Statement: after a successful `Update`, `tracking_floor` is exactly the maximum of its value before and the update's tracking revision, and a public update leaves it unchanged.
  - Location: `program-libs/interface/src/accounts/storage.rs:190-193, 210-212` (`fn StorageRegistry::update`)
  - Severity: Critical (a lower floor lets an enrolled owner prove its salted entry absent)
  - Suggested test: positive with a real private proof, harness: LiteSVM storage

- [x] **INV-ST-UPDATE-21: a refused update leaves the registry unchanged**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_failed_proof_leaves_the_full_history_unchanged`, `a_private_write_refuses_a_missing_or_substituted_tracking_registry`, `a_private_write_names_a_tracking_root_the_registry_serves`, `private_proof_failure_and_stale_revision_keep_the_root_and_floor`, `public_writes_refuse_an_extra_tracking_account`, `a_tracking_revision_follows_exactly_a_private_owner_write`, `each_mode_reaches_its_own_apply_key` (LiteSVM storage)
  - Kind: rollback
  - Statement: when `Update` returns Err, every byte of the registry equals its value before.
  - Location: `programs/storage/src/instructions/update.rs:31-105` (`fn process_update_ix`, the root moves only at 103 after the proof)
  - Severity: Critical
  - Suggested test: negative per refusal with a byte compare, harness: LiteSVM storage

- [ ] **INV-ST-UPDATE-22: an update changes the policy registry alone**
  - Partial coverage: refusals compare the registry, no test compares the tracking registry or other accounts after a landed update.
  - Kind: frame
  - Statement: after every `Update`, the tracking registry and every other account are unchanged, and the registry's `authority`, `grace_slots`, `ring_id` and `tracking_registry` are unchanged.
  - Location: `program-libs/interface/src/accounts/storage.rs:183-215` (`fn StorageRegistry::update`)
  - Severity: Medium
  - Suggested test: positive with an account snapshot, harness: LiteSVM storage

- [ ] **INV-ST-UPDATE-23: the record semantics of an update live in the circuit**
  - Not testable here. INSUFFICIENT_INFO: the sorted tree, the list bits outside each mask, the mint settings move under the entry flag and the exact restrictive classification live in `prover/circuits/registry/apply_records.go`, outside the analyzed source.
  - Kind: state
  - Statement: the storage program checks no record value against the tree, and admits exactly the root the apply proof binds.
  - Location: `programs/storage/src/instructions/update.rs:13-16` (module contract)
  - Severity: Critical
  - Suggested test: none in this crate, harness: Go circuit tests

## Storage UpdateAuthority

### Authorization and Data

- [ ] **INV-ST-UPDATE-AUTH-01: the hand-over takes no data**
  - Not covered.
  - Kind: precondition
  - Statement: every `UpdateAuthority` with a data byte after the tag returns Err.
  - Location: `programs/storage/src/instructions/update_authority.rs:17-19` (`fn process_update_authority_ix`)
  - Error: `StorageError::InvalidInstructionData = 9005`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM storage

- [ ] **INV-ST-UPDATE-AUTH-02: both keys sign**
  - Not covered. The test signs both.
  - Kind: precondition
  - Statement: every `UpdateAuthority` whose current or new authority is not a signer returns Err.
  - Location: `programs/storage/src/instructions/update_authority.rs:23-24` (`fn process_update_authority_ix`)
  - Error: `StorageError::MissingSignature = 9011`
  - Severity: High (a mistyped key would lock the registry)
  - Suggested test: negative per key, harness: LiteSVM storage

- [x] **INV-ST-UPDATE-AUTH-03: only the current authority hands over**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `authority_rotates_and_the_old_authority_is_refused` (LiteSVM storage)
  - Kind: precondition
  - Statement: every `UpdateAuthority` whose first account is not the registry's `authority` returns Err.
  - Location: `programs/storage/src/instructions/update_authority.rs:26` (`fn process_update_authority_ix`)
  - Error: `StorageError::Unauthorized = 9001`
  - Severity: Critical
  - Suggested test: negative, harness: LiteSVM storage

### Success and Frame

- [x] **INV-ST-UPDATE-AUTH-04: the new key holds every right and the old one none**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `authority_rotates_and_the_old_authority_is_refused` (the old key meets `Unauthorized` on hand-over and update, the new key reaches the proof) (LiteSVM storage)
  - Kind: postcondition
  - Statement: after a successful `UpdateAuthority`, `authority` is exactly the new key, and every later `Update` or `UpdateAuthority` signed by the old key returns Err.
  - Location: `programs/storage/src/instructions/update_authority.rs:27` (`fn process_update_authority_ix`)
  - Severity: Critical
  - Suggested test: positive then negative, harness: LiteSVM storage

- [ ] **INV-ST-UPDATE-AUTH-05: a tracking registry's authority never moves**
  - Not covered on `UpdateAuthority`. `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_registry_takes_its_canonical_address_and_loads_owned_writable_and_initialized` refuses a tracking registry in the registry slot of `Update` through the same `load_registry_mut` (LiteSVM storage).
  - Kind: reachability
  - Statement: every `UpdateAuthority` whose registry account is a `TrackingRegistry` returns Err, so `ring_auth` stays the authority of every ring's tracking registry.
  - Location: `programs/storage/src/instructions/update_authority.rs:25` (`fn process_update_authority_ix`), `programs/storage/src/loader.rs:169-183` (`fn load_registry_mut`)
  - Error: `StorageError::InvalidRegistry = 9000`
  - Severity: Critical (a moved authority would apply enrollments past the enrollment writer)
  - Suggested test: negative, harness: LiteSVM storage

- [ ] **INV-ST-UPDATE-AUTH-06: a hand-over moves no root**
  - Not covered.
  - Kind: frame
  - Statement: after every `UpdateAuthority`, every registry field other than `authority` is unchanged.
  - Location: `programs/storage/src/instructions/update_authority.rs:12-29` (`fn process_update_authority_ix`)
  - Severity: Medium
  - Suggested test: positive with a field compare, harness: LiteSVM storage

## Storage OwnerGrant

### Accounts and Authorization

- [ ] **INV-ST-GRANT-01: the instruction names five accounts**
  - Not covered.
  - Kind: precondition
  - Statement: every `OwnerGrant` with fewer than five accounts returns Err.
  - Location: `programs/storage/src/instructions/owner_grant.rs:27-31` (`fn process_owner_grant_ix`)
  - Error: `StorageError::MissingAccounts = 9010`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM storage

- [x] **INV-ST-GRANT-02: the owner signs its own grant**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_grant_refuses_a_stranger_and_a_malformed_reader` (unsigned owner) (LiteSVM storage)
  - Kind: precondition
  - Statement: every `OwnerGrant` whose payer or owner is not a signer returns Err.
  - Location: `programs/storage/src/instructions/owner_grant.rs:32-33` (`fn process_owner_grant_ix`)
  - Error: `StorageError::MissingSignature = 9011`
  - Severity: High (a reader grant opens the owner's activity)
  - Suggested test: negative per account, harness: LiteSVM storage

- [ ] **INV-ST-GRANT-03: the fifth account is the system program**
  - Not covered.
  - Kind: precondition
  - Statement: every `OwnerGrant` whose fifth account is not the system program id returns Err.
  - Location: `programs/storage/src/instructions/owner_grant.rs:34-36` (`fn process_owner_grant_ix`)
  - Error: `StorageError::InvalidSystemProgram = 9013`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM storage

- [x] **INV-ST-GRANT-04: a grant belongs to a tracking registry**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_grant_refuses_a_stranger_and_a_malformed_reader` (a policy registry in the slot) (LiteSVM storage)
  - Kind: precondition
  - Statement: every `OwnerGrant` whose third account is not a program-owned `TrackingRegistry` returns Err.
  - Location: `programs/storage/src/instructions/owner_grant.rs:38` (`fn process_owner_grant_ix`), `programs/storage/src/loader.rs:125-135` (`fn load_tracking_registry`)
  - Error: `StorageError::InvalidTrackingRegistry = 9022`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM storage

### Instruction Data Validation

- [ ] **INV-ST-GRANT-05: the data is exactly 35 bytes**
  - Not covered.
  - Kind: precondition
  - Statement: every `OwnerGrant` whose data after the tag is not exactly `OwnerGrantIxData::SIZE` (35) bytes returns Err.
  - Location: `programs/storage/src/instructions/owner_grant.rs:26` (`fn process_owner_grant_ix`)
  - Error: `StorageError::InvalidInstructionData = 9005`
  - Severity: Medium
  - Suggested test: negative at 34 and 36, harness: LiteSVM storage

- [x] **INV-ST-GRANT-06: the reader key names a scheme in one canonical form**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_grant_refuses_a_stranger_and_a_malformed_reader` (an unknown scheme) (LiteSVM storage), `program-libs/interface/src/accounts/owner_grant.rs` `p256_reader_key_requires_compressed_sec1_prefix` (unit)
  - Kind: precondition
  - Statement: every `OwnerGrant` returns Err unless its reader key is an Ed25519 key (byte 0 is 1, byte 33 is 0) or a compressed P-256 key (byte 0 is 0, byte 1 is 2 or 3).
  - Location: `program-libs/interface/src/accounts/owner_grant.rs:37-45` (`fn OwnerGrantIxData::check`)
  - Error: `StorageError::InvalidReaderKey = 9032`
  - Severity: Medium (one reader takes one grant address)
  - Suggested test: negative per scheme byte, harness: LiteSVM storage

- [x] **INV-ST-GRANT-07: the active flag is 0 or 1**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_grant_refuses_a_stranger_and_a_malformed_reader` (LiteSVM storage)
  - Kind: precondition
  - Statement: every `OwnerGrant` whose `active` byte is above 1 returns Err.
  - Location: `program-libs/interface/src/accounts/owner_grant.rs:46-48` (`fn OwnerGrantIxData::check`)
  - Error: `StorageError::InvalidInstructionData = 9005`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM storage

### Create and Toggle

- [x] **INV-ST-GRANT-08: a grant sits at the address of its registry, owner and reader**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_grant_refuses_a_stranger_and_a_malformed_reader` (another owner's address, the registry's address), `owner_grants_are_bound_to_their_tracking_registry` (LiteSVM storage)
  - Kind: precondition
  - Statement: every `OwnerGrant` whose grant account is not the writable PDA of `[b"owner_grant", registry, owner, reader[0..2], reader[2..34]]` returns Err.
  - Location: `programs/storage/src/loader.rs:94-113` (`fn OwnerGrantWrite::write`), `program-libs/interface/src/accounts/owner_grant.rs:25-32` (`fn OwnerGrant::seeds`)
  - Error: `StorageError::InvalidOwnerGrant = 9031`
  - Severity: High (no signer writes another owner's grant)
  - Suggested test: negative per seed, harness: LiteSVM storage

- [x] **INV-ST-GRANT-09: the first call creates the grant and later calls toggle it**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `an_owner_grants_and_revokes_a_reader_alone`, `a_program_owner_writes_its_grant_through_cpi` (LiteSVM storage)
  - Kind: postcondition
  - Statement: after a successful `OwnerGrant` over an empty account, the grant is exactly `{ discriminator: 5, active }` at 2 bytes, and after one over an existing grant only `active` changes, to exactly the requested value.
  - Location: `programs/storage/src/loader.rs:102-120` (`fn OwnerGrantWrite::write`)
  - Severity: Medium
  - Suggested test: positive grant, revoke, grant, harness: LiteSVM storage

- [ ] **INV-ST-GRANT-10: an existing grant account is a program grant**
  - Not covered.
  - Kind: precondition
  - Statement: every `OwnerGrant` over a nonempty account at the canonical address that is not program-owned or not a 2-byte grant of discriminator 5 returns Err.
  - Location: `programs/storage/src/loader.rs:114-119` (`fn OwnerGrantWrite::write`)
  - Error: `StorageError::InvalidOwnerGrant = 9031`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM storage

- [ ] **INV-ST-GRANT-11: a grant moves no funds**
  - Not covered.
  - Kind: state
  - Statement: no instruction of the ring program or the storage program reads an `OwnerGrant` account.
  - Location: `programs/storage/src/instructions/owner_grant.rs:9-10` (module contract)
  - Severity: Medium
  - Suggested test: property by code search in CI, harness: none
  - SPEC_DIVERGENCE: `programs/storage/src/instructions/owner_grant.rs:21-47` accepts every signing owner, a wallet key included, vs `docs/spec.md` Accounts item 6 and Instructions, Storage Program item 5. The spec scopes the grant to "a program owner's activity". The code holds no off-curve check on the owner.

- [ ] **INV-ST-GRANT-12: a grant changes the grant account alone**
  - Not covered.
  - Kind: frame
  - Statement: after every `OwnerGrant`, the tracking registry is unchanged and only the payer and the grant account move lamports or data.
  - Location: `programs/storage/src/instructions/owner_grant.rs:21-47` (`fn process_owner_grant_ix`)
  - Severity: Medium
  - Suggested test: positive with an account snapshot, harness: LiteSVM storage

## Storage EmitEvent

- [x] **INV-ST-EVENT-01: the event tag accepts every input and writes nothing**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `the_event_tag_accepts_any_bytes_and_writes_nothing` (LiteSVM storage)
  - Kind: postcondition
  - Statement: every storage instruction with tag 10 returns Ok for every account list and every data, and changes no account.
  - Location: `programs/storage/src/lib.rs:62` (`fn process_instruction`)
  - Severity: Medium
  - Suggested test: positive with forged bytes and an account snapshot, harness: LiteSVM storage

- [ ] **INV-ST-EVENT-02: an event counts only under a successful apply**
  - Not testable here. INSUFFICIENT_INFO: the rule that a reader trusts a `StorageEvent` only as the inner instruction of a successful storage `ApplyEnrollments` lives in the ring indexer, outside the analyzed source.
  - Kind: state
  - Statement: a top-level or forged tag 10 instruction carries no authority over the tracking tree a reader rebuilds.
  - Location: `program-libs/interface/src/instructions/storage.rs:166-173` (`struct StorageEvent`)
  - Severity: High
  - Suggested test: none in this crate, harness: indexer suite
