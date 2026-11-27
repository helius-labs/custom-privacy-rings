# Enrollment Invariants

Covers the ring program's `EnrollBatch` (tag 66) and the storage program's `InitializeTracking` (tag 9) and `ApplyEnrollments` (tag 7).

A ring's tracking registry has `ring_auth` as its authority, so every write reaches it through `EnrollBatch`. The ring decides who submits, storage checks every owner signature, every `RegisterKey` seal and the apply proof. The shared root history rule (`advance_roots`, `root_at`), the canonical PDA creation and the storage dispatch live in `cross-cutting.md`.

## EnrollBatch

### Authorization and Accounts

- [x] **INV-ENROLL-01: only the enrollment writer submits**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `only_the_enrollment_writer_reaches_storage_through_the_ring_pda` (the policy authority) (LiteSVM)
  - Kind: precondition
  - Statement: every `EnrollBatch` whose first account is not a signer holding the `EnrollmentWriter` role returns Err before the storage CPI.
  - Location: `programs/custom-ring/src/instructions/tracking/enroll_batch.rs:22, 30-35` (`fn process_enroll_batch_ix`)
  - Error: `CustomRingError::UnauthorizedAuthority = 8107`
  - Severity: Critical (`ring_auth` signs as the tracking registry's authority)
  - Suggested test: negative per other role, harness: LiteSVM

- [x] **INV-ENROLL-02: the batch goes to the registry the ring pins**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `only_the_enrollment_writer_reaches_storage_through_the_ring_pda` (LiteSVM)
  - Kind: precondition
  - Statement: every `EnrollBatch` whose `tracking_registry` account differs from the ring's `tracking` pin returns Err before the storage CPI.
  - Location: `programs/custom-ring/src/instructions/tracking/enroll_batch.rs:36-38` (`fn process_enroll_batch_ix`)
  - Error: `CustomRingError::InvalidTrackingRegistry = 8502`
  - Severity: Critical (the ring PDA would sign for another registry it created)
  - Suggested test: negative, harness: LiteSVM

- [ ] **INV-ENROLL-03: the `ring_auth` account is the ring PDA**
  - Not covered.
  - Kind: precondition
  - Statement: every `EnrollBatch` whose `ring_auth` account differs from the PDA of `[RING_AUTH_PDA_SEED]` returns Err before the storage CPI.
  - Location: `programs/custom-ring/src/instructions/tracking/enroll_batch.rs:41` (`fn process_enroll_batch_ix`)
  - Error: `CustomRingError::MissingRingAuth = 8104`
  - Severity: Medium (storage would refuse the authority, `INV-ST-APPLY-05`)
  - Suggested test: negative, harness: LiteSVM

- [ ] **INV-ENROLL-04: the tracking account is writable**
  - Not covered.
  - Kind: precondition
  - Statement: every `EnrollBatch` whose `tracking_registry` account is read-only returns Err.
  - Location: `programs/custom-ring/src/instructions/tracking/enroll_batch.rs:25` (`fn process_enroll_batch_ix`)
  - Error: `AccountError::AccountNotMutable = 20002`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM

### CPI

- [x] **INV-ENROLL-05: the ring forwards the batch unchanged under `ring_auth`**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `only_the_enrollment_writer_reaches_storage_through_the_ring_pda` (storage accepts `ring_auth` and stops at the signature check), `pda_enrollment_and_rotation_require_the_vault_manager` (an enroll and a rotation land) (LiteSVM)
  - Kind: postcondition
  - Statement: a successful `EnrollBatch` makes exactly one CPI, to the storage program, with data exactly `APPLY_ENROLLMENTS || data` and accounts exactly `[ring_auth, tracking_registry, owner_authorization, storage_program]`, `ring_auth` raised to signer.
  - Location: `programs/custom-ring/src/instructions/tracking/enroll_batch.rs:42-52` (`fn process_enroll_batch_ix`)
  - Severity: Critical
  - Suggested test: positive with inner instruction inspection, harness: LiteSVM

- [x] **INV-ENROLL-06: the ring lends no signature to the owner slot**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `pda_enrollment_requires_one_owner_signature_per_invocation` (an unsigned PDA owner meets `MissingSignature`, the writer and `ring_auth` as owners meet `OwnerSignatureMismatch`) (LiteSVM)
  - Kind: precondition
  - Statement: the forwarded `owner_authorization` carries exactly the signer flag it held on entry, except when it is `ring_auth`. The CPI raises every copy of the PDA to signer. Storage then refuses every write whose owner is the registry authority, so a raised `ring_auth` enrolls nothing.
  - Location: `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs:168-175` (`fn PlacedRingAuth::invoke_program`), `program-libs/interface/src/instructions/storage.rs:282-287` (`fn EnrollmentWrite::check_pda_owner`)
  - Error: `StorageError::MissingSignature = 9011` / `StorageError::OwnerSignatureMismatch = 9043`
  - Severity: Critical (the writer must never enroll a key its owner did not sign)
  - Suggested test: negative per owner kind, harness: LiteSVM

### Frame

- [ ] **INV-ENROLL-07: the ring program writes nothing of its own**
  - Not covered.
  - Kind: frame
  - Statement: after every `EnrollBatch`, the ring account is unchanged and the only account the call changes is the tracking registry, through storage.
  - Location: `programs/custom-ring/src/instructions/tracking/enroll_batch.rs:16-53` (`fn process_enroll_batch_ix`)
  - Severity: Medium
  - Suggested test: positive with an account snapshot, harness: LiteSVM

- [ ] **INV-ENROLL-08: enrollment runs while the ring is paused**
  - Not covered.
  - Kind: reachability
  - Statement: for every ring whose SPP `RingConfig.paused` is set, an otherwise valid `EnrollBatch` succeeds. Neither program reads the pause flag on the enrollment path.
  - Location: `programs/custom-ring/src/instructions/tracking/enroll_batch.rs:16-53` (`fn process_enroll_batch_ix`)
  - Severity: Medium
  - Suggested test: positive after `SetPaused`, harness: localnet

## Storage InitializeTracking

### Accounts and Authorization

- [ ] **INV-ST-INIT-TRACK-01: the instruction names four accounts**
  - Not covered.
  - Kind: precondition
  - Statement: every `InitializeTracking` with fewer than four accounts returns Err.
  - Location: `programs/storage/src/instructions/initialize_tracking.rs:28-31` (`fn process_initialize_tracking_ix`)
  - Error: `StorageError::MissingAccounts = 9010`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM storage

- [x] **INV-ST-INIT-TRACK-02: the payer and the creator sign**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_tracking_registry_pins_ring_and_auditor_at_its_canonical_address` (unsigned creator) (LiteSVM storage)
  - Kind: precondition
  - Statement: every `InitializeTracking` whose payer or creator is not a signer returns Err.
  - Location: `programs/storage/src/instructions/initialize_tracking.rs:32-33` (`fn process_initialize_tracking_ix`)
  - Error: `StorageError::MissingSignature = 9011`
  - Severity: High (the creator's key in the seeds keeps every other signer off the address)
  - Suggested test: negative per account, harness: LiteSVM storage

- [ ] **INV-ST-INIT-TRACK-03: the fourth account is the system program**
  - Not covered.
  - Kind: precondition
  - Statement: every `InitializeTracking` whose fourth account is not the system program id returns Err.
  - Location: `programs/storage/src/instructions/initialize_tracking.rs:34-36` (`fn process_initialize_tracking_ix`)
  - Error: `StorageError::InvalidSystemProgram = 9013`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM storage

### Instruction Data Validation

- [ ] **INV-ST-INIT-TRACK-04: the data is exactly 65 bytes**
  - Not covered.
  - Kind: precondition
  - Statement: every `InitializeTracking` whose data after the tag is not exactly `InitializeTrackingIxData::SIZE` (65) bytes returns Err.
  - Location: `programs/storage/src/instructions/initialize_tracking.rs:27` (`fn process_initialize_tracking_ix`), `programs/storage/src/lib.rs:82-84` (`fn parse`)
  - Error: `StorageError::InvalidInstructionData = 9005`
  - Severity: Medium
  - Suggested test: negative at 64 and 66, harness: LiteSVM storage

- [ ] **INV-ST-INIT-TRACK-05: the ring id is a nonzero canonical field value**
  - Not covered. Every test uses a canonical nonzero id.
  - Kind: precondition
  - Statement: every `InitializeTracking` whose ring id is zero or not a canonical BN254 scalar returns Err.
  - Location: `program-libs/interface/src/accounts/tracking.rs:205-208` (`fn InitializeTrackingIxData::check`)
  - Error: `StorageError::InvalidTrackingRegistry = 9022`
  - Severity: Medium
  - Suggested test: negative at zero and at the modulus, harness: LiteSVM storage

- [x] **INV-ST-INIT-TRACK-06: the auditor key is tagged, canonical and of large order**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_tracking_registry_pins_ring_and_auditor_at_its_canonical_address` (untagged, low order) (LiteSVM storage)
  - Kind: precondition
  - Statement: every `InitializeTracking` whose auditor key fails `is_auditor_key` returns Err.
  - Location: `program-libs/interface/src/accounts/tracking.rs:209-211` (`fn InitializeTrackingIxData::check`)
  - Error: `StorageError::InvalidAuditorKey = 9033`
  - Severity: High (no enrollment could ever seal to the key)
  - Suggested test: negative per low order point, harness: LiteSVM storage

### Create

- [x] **INV-ST-INIT-TRACK-07: the registry sits at the creator's canonical PDA**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_tracking_registry_pins_ring_and_auditor_at_its_canonical_address` (an address of another ring id) (LiteSVM storage), `program-tests/custom-ring/tests/rules/operator_rules.rs` `initialize_refuses_a_tracking_account_off_its_seeds` (LiteSVM)
  - Kind: precondition
  - Statement: every `InitializeTracking` whose registry account is not the writable PDA of `[b"tracking_registry", creator, ring_id]` returns Err.
  - Location: `programs/storage/src/loader.rs:66-73` (`fn TrackingRegistryInit::init`), `programs/storage/src/loader.rs:338-345` (`fn CanonicalPda::bump`)
  - Error: `StorageError::InvalidTrackingRegistry = 9022`
  - Severity: High
  - Suggested test: negative, harness: LiteSVM storage

- [x] **INV-ST-INIT-TRACK-08: a registry is created once**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_tracking_registry_pins_ring_and_auditor_at_its_canonical_address` (LiteSVM storage)
  - Kind: precondition
  - Statement: every `InitializeTracking` over an account that holds data returns Err.
  - Location: `programs/storage/src/loader.rs:351-353` (`fn CanonicalPda::create`)
  - Error: `StorageError::AlreadyInitialized = 9014`
  - Severity: High
  - Suggested test: negative, harness: LiteSVM storage

- [x] **INV-ST-INIT-TRACK-09: a new registry holds the sentinel root at revision zero**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_tracking_registry_pins_ring_and_auditor_at_its_canonical_address` (LiteSVM storage), `sdk-libs/client/tests/accounts.rs` `a_tracking_registry_starts_at_the_sentinel_and_pins_ring_and_auditor` (library)
  - Kind: postcondition
  - Statement: after a successful `InitializeTracking`, the registry is exactly `TrackingRegistry::SIZE` (6250) bytes with discriminator 3, `authority` equal to the creator, the requested ring id and auditor key, `next_index` 1, `roots[0]` equal to `TRACKING_EMPTY_ROOT` at revision 0 and every other root entry zero.
  - Location: `programs/storage/src/loader.rs:73-77` (`fn TrackingRegistryInit::init`), `program-libs/interface/src/accounts/tracking.rs:113-120` (`fn TrackingRegistry::init`)
  - Severity: High
  - Suggested test: positive, harness: LiteSVM storage

- [ ] **INV-ST-INIT-TRACK-10: the ring id, the auditor key and the authority are final**
  - Partial coverage: no instruction writes them, see `INV-ST-UPDATE-AUTH-05` for the authority. No test tries every storage instruction against a tracking registry.
  - Kind: state
  - Statement: after `InitializeTracking`, no storage instruction changes the registry's `ring_id`, `auditor_pubkey` or `authority`.
  - Location: `program-libs/interface/src/accounts/tracking.rs:135-152` (`fn TrackingRegistry::update`, the one writer after `init`)
  - Severity: Critical (every first enrollment seals to this key)
  - Suggested test: property over every storage instruction, harness: LiteSVM storage

- [ ] **INV-ST-INIT-TRACK-11: a creation changes the payer and the new registry alone**
  - Not covered.
  - Kind: frame
  - Statement: after every successful `InitializeTracking`, every account other than the payer and the registry is unchanged, and the payer's lamports decrease by exactly the rent the registry takes beyond a donated balance.
  - Location: `programs/storage/src/loader.rs:349-368` (`fn CanonicalPda::create`)
  - Severity: Medium
  - Suggested test: positive with lamport deltas, harness: LiteSVM storage

## Storage ApplyEnrollments

### Instruction Data Validation

- [x] **INV-ST-APPLY-01: a batch names 1 to 5 writes and exactly that many follow**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `an_apply_refuses_a_write_no_proof_or_signature_binds` (count 0 and 6) (LiteSVM storage), `sdk-libs/client/tests/accounts.rs` `a_request_parses_exactly_its_count_of_writes`, `an_apply_names_one_to_a_full_batch_of_writes` (library)
  - Kind: precondition
  - Statement: every `ApplyEnrollments` returns `NoEnrollmentsNamed` for a count of 0, `BatchTooLarge` for a count above `ENROLLMENT_BATCH` (5), and `InvalidInstructionData` when the data holds an unknown write kind or a byte past the counted writes.
  - Location: `program-libs/interface/src/instructions/storage.rs:343-351` (`fn ApplyEnrollmentsRequest::parse`), `storage.rs:362-368` (`fn check_count`), `storage.rs:499-512` (`fn parse_writes`), `storage.rs:235-251` (`fn EnrollmentWrite::parse`)
  - Error: `StorageError::NoEnrollmentsNamed = 9030` / `StorageError::BatchTooLarge = 9006` / `StorageError::InvalidInstructionData = 9005`
  - Severity: Medium
  - Suggested test: negative per case, harness: LiteSVM storage

### Accounts and Authorization

- [ ] **INV-ST-APPLY-02: the instruction names four accounts**
  - Not covered.
  - Kind: precondition
  - Statement: every `ApplyEnrollments` with fewer than four accounts returns Err.
  - Location: `programs/storage/src/instructions/apply_enrollments.rs:44-47` (`fn process_apply_enrollments_ix`)
  - Error: `StorageError::MissingAccounts = 9010`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM storage

- [x] **INV-ST-APPLY-03: the fourth account is the storage program**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `an_apply_needs_the_registry_authority_and_the_storage_program` (LiteSVM storage)
  - Kind: precondition
  - Statement: every `ApplyEnrollments` whose fourth account is not the storage program id returns Err.
  - Location: `programs/storage/src/instructions/apply_enrollments.rs:50` (`fn process_apply_enrollments_ix`), `programs/storage/src/loader.rs:308-317` (`fn check_storage_program`)
  - Error: `StorageError::InvalidStorageProgram = 9041`
  - Severity: Medium (the event self CPI resolves the program from this account)
  - Suggested test: negative, harness: LiteSVM storage

- [x] **INV-ST-APPLY-04: the registry is a writable program tracking registry**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `an_apply_needs_the_registry_authority_and_the_storage_program` (a policy registry in the slot) (LiteSVM storage)
  - Kind: precondition
  - Statement: every `ApplyEnrollments` whose second account is not a program-owned, writable `TrackingRegistry` of exact size, discriminator 3 and `next_index` in 1 to `TRACKING_CAPACITY` returns Err.
  - Location: `programs/storage/src/loader.rs:156-167` (`fn load_tracking_registry_mut`), `loader.rs:206-220` (`fn ProgramAccount::load_mut`)
  - Error: `StorageError::InvalidTrackingRegistry = 9022`
  - Severity: High
  - Suggested test: negative per field, harness: LiteSVM storage

- [x] **INV-ST-APPLY-05: only the registry's authority applies**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `an_apply_needs_the_registry_authority_and_the_storage_program` (unsigned, another signer) (LiteSVM storage), `program-tests/custom-ring/tests/rules/operator_rules.rs` `pda_enrollment_and_rotation_require_the_vault_manager` (LiteSVM)
  - Kind: precondition
  - Statement: every `ApplyEnrollments` whose first account does not sign returns `MissingSignature`, and every one whose first account is not the registry's `authority` returns `Unauthorized`.
  - Location: `programs/storage/src/instructions/apply_enrollments.rs:49, 53-55` (`fn process_apply_enrollments_ix`)
  - Error: `StorageError::MissingSignature = 9011` / `StorageError::Unauthorized = 9001`
  - Severity: Critical
  - Suggested test: negative both legs, harness: LiteSVM storage

### Owner Consent

- [x] **INV-ST-APPLY-06: a batch reads its signatures from the instruction right before it**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_batch_applies_only_what_each_owner_signed_over_its_registry` (no precompile, another signature count) (LiteSVM storage), `program-tests/custom-ring/tests/rules/operator_rules.rs` `only_the_enrollment_writer_reaches_storage_through_the_ring_pda` (LiteSVM)
  - Kind: precondition
  - Statement: when `owner_authorization` is the instructions sysvar, every `ApplyEnrollments` whose preceding top-level instruction is not the Ed25519 precompile, or names a signature count other than the write count, returns Err.
  - Location: `programs/storage/src/loader.rs:264-274` (`fn EnrollmentOwners::authorize`), `program-libs/interface/src/proof/owner_signature.rs:42-55` (`fn Ed25519Signatures::parse`)
  - Error: `StorageError::InvalidSignatureInstruction = 9042`
  - Severity: Critical
  - Suggested test: negative per case, harness: LiteSVM storage

- [x] **INV-ST-APPLY-07: each write carries its owner's signature over its registry and entry**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `a_batch_applies_only_what_each_owner_signed_over_its_registry` (swapped owners, another registry), `an_owner_signs_the_enrollment_text_and_a_binary_message_is_refused` (LiteSVM storage), `sdk-libs/client/tests/accounts.rs` `a_signature_matches_only_its_own_owner_and_message_inside_the_precompile` (library)
  - Kind: precondition
  - Statement: for every write `i`, descriptor `i` must point every field into the precompile instruction itself, name the write's owner as its key, and sign exactly the text over the registry address, the entry hash and the action. Every other descriptor returns Err.
  - Location: `programs/storage/src/loader.rs:295-302` (`fn EnrollmentAuthorization::entry_hash`), `program-libs/interface/src/proof/owner_signature.rs:58-89` (`fn Ed25519Signatures::check`), `program-libs/interface/src/instructions/storage.rs:307-338` (`fn EnrollmentWrite::signed`)
  - Error: `StorageError::OwnerSignatureMismatch = 9043`
  - Severity: Critical (the writer submits only what each owner signed)
  - Suggested test: negative per descriptor field, harness: LiteSVM storage

- [x] **INV-ST-APPLY-08: a PDA owner signs one write of its own**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `pda_enrollment_requires_one_owner_signature_per_invocation` (unsigned PDA, a keypair owner, the writer and `ring_auth` as owners, two writes) (LiteSVM)
  - Kind: precondition
  - Statement: when `owner_authorization` is not the instructions sysvar, `ApplyEnrollments` returns `MissingSignature` unless it signs, `UnsupportedBatch` unless the batch holds exactly one write, and `OwnerSignatureMismatch` unless the write's owner is that signer, differs from the registry authority and lies off the Ed25519 curve.
  - Location: `programs/storage/src/loader.rs:254-263` (`fn EnrollmentOwners::authorize`), `loader.rs:285-293` (`fn EnrollmentAuthorization::entry_hash`), `program-libs/interface/src/instructions/storage.rs:282-287` (`fn check_pda_owner`)
  - Error: `StorageError::MissingSignature = 9011` / `StorageError::UnsupportedBatch = 9016` / `StorageError::OwnerSignatureMismatch = 9043`
  - Severity: Critical
  - Suggested test: negative per case, harness: LiteSVM

### Write Checks

- [x] **INV-ST-APPLY-09: every viewing key is nonzero and canonical**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `an_apply_refuses_a_write_no_proof_or_signature_binds` (a zero rotation key) (LiteSVM storage)
  - Kind: precondition
  - Statement: every write whose viewing key is zero or not a canonical X25519 coordinate returns Err.
  - Location: `program-libs/interface/src/instructions/storage.rs:275-280` (`fn EnrollmentWrite::check`), `program-libs/interface/src/accounts/tracking.rs:233-238` (`fn check_viewing_key`)
  - Error: `StorageError::NonCanonicalViewingKey = 9021`
  - Severity: High (a sender would seal to a key that opens nothing)
  - Suggested test: negative per write kind, harness: LiteSVM storage

- [x] **INV-ST-APPLY-10: no write enrolls the zero nullifier key**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `an_apply_refuses_a_write_no_proof_or_signature_binds` (LiteSVM storage), `sdk-libs/client/tests/accounts.rs` `the_enrollment_checks_what_no_proof_binds_and_refuses_a_zero_key` (library)
  - Kind: precondition
  - Statement: every enroll write whose nullifier key is `ZERO_NULLIFIER_PUBKEY` returns Err.
  - Location: `program-libs/interface/src/accounts/tracking.rs:220-222` (`fn EnrollIxData::check`)
  - Error: `StorageError::ZeroNullifierKey = 9028`
  - Severity: High (a zero key's row would read as present)
  - Suggested test: negative, harness: LiteSVM storage

- [ ] **INV-ST-APPLY-11: the enrolled nullifier key and list tag are canonical field values**
  - Partial coverage: `sdk-libs/client/tests/accounts.rs` `the_enrollment_checks_what_no_proof_binds_and_refuses_a_zero_key` (library). No on-chain test.
  - Kind: precondition
  - Statement: every enroll write whose nullifier key or list tag is not a canonical BN254 scalar returns Err.
  - Location: `program-libs/interface/src/accounts/tracking.rs:223-227` (`fn EnrollIxData::check`)
  - Error: `StorageError::InvalidInstructionData = 9005`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM storage

- [x] **INV-ST-APPLY-12: every first enrollment proves its seal to the registry's auditor**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `an_apply_refuses_a_write_no_proof_or_signature_binds` (a zero proof) (LiteSVM storage), `program-tests/custom-ring/tests/rules/operator_rules.rs` `pda_enrollment_and_rotation_require_the_vault_manager` (a real proof lands) (LiteSVM)
  - Kind: precondition
  - Statement: every enroll write returns Err unless its `RegisterKey` proof verifies over the registry's `ring_id`, the owner's member identity, the write's nullifier key, list tag, ephemeral key, ciphertext hash and the registry's `auditor_pubkey`.
  - Location: `programs/storage/src/instructions/apply_enrollments.rs:68-77` (`fn process_apply_enrollments_ix`), `program-libs/interface/src/proof/verification.rs:57-78` (`fn RegisterKeyCheck::verify`), `program-libs/interface/src/proof/statement.rs:468-483` (`fn RegisterKeyStatement::elements`)
  - Error: `StorageError::ProofVerificationFailed = 9008`
  - Severity: Critical (a false seal would hide an owner's nullifier secret from the auditor)
  - Suggested test: negative per statement element with a real proof, harness: LiteSVM

- [ ] **INV-ST-APPLY-13: a rotation's sequence number stays in u64**
  - Not covered.
  - Kind: precondition
  - Statement: every rotation write whose prior sequence number is `u64::MAX` returns Err.
  - Location: `program-libs/interface/src/accounts/tracking.rs:175` (`fn TrackingEntry::from_rotation`)
  - Error: `StorageError::InvalidInstructionData = 9005`
  - Severity: Medium
  - Suggested test: negative, harness: library

### Apply Proof and Root

- [x] **INV-ST-APPLY-14: the apply names the registry's current revision**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `an_apply_refuses_a_write_no_proof_or_signature_binds` (revision 1 on a rotation) (LiteSVM storage)
  - Kind: precondition
  - Statement: every `ApplyEnrollments` whose revision differs from the newest root's revision returns Err.
  - Location: `programs/storage/src/instructions/apply_enrollments.rs:85-90` (`fn process_apply_enrollments_ix`), `program-libs/interface/src/accounts/storage.rs:261-263` (`fn advance_roots`)
  - Error: `StorageError::RevisionChanged = 9002`
  - Severity: High
  - Suggested test: negative, harness: LiteSVM storage

- [x] **INV-ST-APPLY-15: the next index stays within the tree**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `an_apply_past_two_to_the_thirty_two_leaves_is_refused` (LiteSVM storage), `sdk-libs/client/tests/accounts.rs` `an_apply_fills_the_last_leaf_and_refuses_an_index_past_it` (library)
  - Kind: precondition
  - Statement: every `ApplyEnrollments` whose new next index is 0 or strictly greater than `TRACKING_CAPACITY` (2^32) returns Err and leaves the registry unchanged.
  - Location: `program-libs/interface/src/accounts/tracking.rs:135-139` (`fn TrackingRegistry::update`)
  - Error: `StorageError::TrackingTreeFull = 9036`
  - Severity: Medium
  - Suggested test: negative at capacity + 1, harness: LiteSVM storage

- [x] **INV-ST-APPLY-16: no apply evicts an unexpired root**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `an_apply_over_unexpired_roots_waits_until_the_oldest_expires` (LiteSVM storage), `sdk-libs/client/tests/accounts.rs` `a_tracking_update_waits_while_its_eviction_would_drop_an_unexpired_root` (library)
  - Kind: precondition
  - Statement: every `ApplyEnrollments` at a slot strictly below the expiry of the oldest kept root returns Err and leaves the registry unchanged, and the same apply passes that check once the slot reaches the expiry.
  - Location: `program-libs/interface/src/accounts/storage.rs:269-271` (`fn advance_roots`, `Retirement::Retained`)
  - Error: `StorageError::RootHistoryRetained = 9037`
  - Severity: High (an in-flight proof under a kept root must still settle)
  - Suggested test: negative then positive at the expiry slot, harness: LiteSVM storage

- [x] **INV-ST-APPLY-17: the apply proof binds the transition and the ordered writes**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `signed_rotations_reach_the_apply_proof_under_the_registry_authority` (a zero proof leaves the root) (LiteSVM storage), `program-tests/custom-ring/tests/rules/operator_rules.rs` `pda_enrollment_and_rotation_require_the_vault_manager` (a real proof lands) (LiteSVM)
  - Kind: precondition
  - Statement: every `ApplyEnrollments` returns Err unless its proof verifies under the `ApplyEnrollments` key of `apply_width(count)` over `[APPLY_ENROLLMENTS, old root, new root, old next index, new next index, chain start, chain end, count]`, with the old values read from the account, the chain started at `(registry, revision)` and linked over every entry hash in write order.
  - Location: `programs/storage/src/instructions/apply_enrollments.rs:61-81, 92-109` (`fn process_apply_enrollments_ix`), `program-libs/interface/src/accounts/tracking.rs:262-267` (`fn tracking_chain_start`), `program-libs/interface/src/proof/statement.rs:433-444` (`fn ApplyEnrollmentsStatement::elements`)
  - Error: `StorageError::ProofVerificationFailed = 9008`
  - Severity: Critical (a root the proof does not bind would hold rows no owner signed)
  - Suggested test: negative per statement element with a real proof, harness: LiteSVM storage

- [x] **INV-ST-APPLY-18: a failed apply proof reverts the root it wrote**
  - Covered by: `program-tests/custom-ring/tests/storage_lifecycle.rs` `signed_rotations_reach_the_apply_proof_under_the_registry_authority`, `an_apply_over_unexpired_roots_waits_until_the_oldest_expires` (the registry equals its value before) (LiteSVM storage)
  - Kind: rollback
  - Statement: when the apply proof fails after the registry's roots and next index were written, every byte of the registry equals its value before the instruction.
  - Location: `programs/storage/src/instructions/apply_enrollments.rs:85-109` (`fn process_apply_enrollments_ix`, the update at 85 precedes the proof at 101)
  - Severity: Critical
  - Suggested test: negative with a byte compare, harness: LiteSVM storage

### Success and Frame

- [x] **INV-ST-APPLY-19: a successful apply moves the root one revision on and keeps the old one for the grace**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `pda_enrollment_and_rotation_require_the_vault_manager` (newest root, revision, next index) (LiteSVM), `sdk-libs/client/tests/accounts.rs` `a_tracking_update_serves_the_old_root_for_its_grace` (library)
  - Kind: postcondition
  - Statement: after a successful `ApplyEnrollments` at slot `s`, `roots[0]` is exactly the new root at revision `r + 1`, the outgoing root sits at `roots[1]` with expiry `s + TRACKING_GRACE_SLOTS` (750), and `next_index` is exactly the new next index.
  - Location: `program-libs/interface/src/accounts/tracking.rs:135-152` (`fn TrackingRegistry::update`), `program-libs/interface/src/accounts/storage.rs:255-286` (`fn advance_roots`)
  - Severity: High
  - Suggested test: positive with the expiry read back, harness: LiteSVM storage

- [ ] **INV-ST-APPLY-20: a successful apply emits one event carrying every write**
  - Partial coverage: `sdk-libs/client/tests/accounts.rs` `an_event_decodes_only_under_its_tag_kind_and_exact_writes` (library) pins the encoding. No on-chain test reads the inner instruction.
  - Kind: postcondition
  - Statement: after a successful `ApplyEnrollments`, the transaction holds exactly one storage self CPI with zero accounts and data `EMIT_EVENT || ENROLLMENTS_APPLIED || { registry, new revision, new root, new next index, count } || every write as sent`, and no event is emitted on a refused apply.
  - Location: `programs/storage/src/instructions/apply_enrollments.rs:111-124` (`fn process_apply_enrollments_ix`), `programs/storage/src/lib.rs:69-80` (`fn emit_event`), `program-libs/interface/src/instructions/storage.rs:375-384` (`fn StorageEvent::encode`)
  - Severity: High (the tree and every sealed secret rebuild from events alone)
  - Suggested test: positive with inner instruction inspection, harness: LiteSVM

- [ ] **INV-ST-APPLY-21: an apply changes the tracking registry alone**
  - Not covered.
  - Kind: frame
  - Statement: after every `ApplyEnrollments`, every field of the registry other than `roots` and `next_index` is unchanged, and no other account changes.
  - Location: `program-libs/interface/src/accounts/tracking.rs:135-152` (`fn TrackingRegistry::update`)
  - Severity: Medium
  - Suggested test: positive with an account snapshot, harness: LiteSVM storage

- [ ] **INV-ST-APPLY-22: the tree semantics of an apply live in the circuit**
  - Not testable here. INSUFFICIENT_INFO: the sorted insertion, the refusal of a present owner's second enroll, the no-op of a rotation for an absent owner and the move of the viewing hash alone live in `prover/circuits/registry/apply_enrollments.go`, outside the analyzed source.
  - Kind: state
  - Statement: the storage program checks no tree position, and admits exactly the root the apply proof binds.
  - Location: `programs/storage/src/instructions/apply_enrollments.rs:11-14` (module contract)
  - Severity: Critical
  - Suggested test: none in this crate, harness: Go circuit tests
