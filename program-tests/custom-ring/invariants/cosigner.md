# Co-signer Invariants

Covers `SetCoSigner` (tag 27) and `ClearCoSigner` (tag 62).

A record sits at `[b"co_signer", signer]`, so one signer holds one record. The ring keeps one count per optional class (`TRANSFERS`, `DEPOSITS`, `WITHDRAWALS`), and an optional class demands a co-signer only while its count is nonzero. The approval classes take no count. The transfer reads the records, see `transfer.md` INV-TRANSFER-25..35. The role check lives in `cross-cutting.md`.

## SetCoSigner

### Authorization and Accounts

- [x] **INV-SET-COSIGNER-01: only the co-signer admin writes a record**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `each_role_instruction_refuses_every_other_role_and_the_upgrade_authority` (LiteSVM)
  - Kind: precondition
  - Statement: every `SetCoSigner` whose second account is not a signer holding the `CosignAdmin` role returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_cosigner.rs:33, 40-45` (`fn process_set_cosigner_ix`)
  - Error: `CustomRingError::UnauthorizedAuthority = 8107`
  - Severity: Critical (a record turns a class's demand on or satisfies it)
  - Suggested test: negative per other role, harness: LiteSVM

- [ ] **INV-SET-COSIGNER-02: the payer signs and the ring and record are writable**
  - Partial coverage: every test signs and marks them writable. No test sends an unsigned payer or a read-only ring or record.
  - Kind: precondition
  - Statement: every `SetCoSigner` whose payer is not a writable signer, or whose ring or record account is read-only, returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_cosigner.rs:32-35` (`fn process_set_cosigner_ix`)
  - Error: `AccountError::InvalidSigner = 20009` / `AccountError::AccountNotMutable = 20002`
  - Severity: Medium
  - Suggested test: negative per account, harness: LiteSVM

- [ ] **INV-SET-COSIGNER-03: the fifth account is the system program**
  - Not covered on `SetCoSigner`.
  - Kind: precondition
  - Statement: every `SetCoSigner` whose `system_program` account is not the system program id returns Err, also on a rescope.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_cosigner.rs:39` (`fn process_set_cosigner_ix`)
  - Error: `CustomRingError::InvalidSystemProgram = 8111`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM

### Instruction Data Validation

- [x] **INV-SET-COSIGNER-04: the scope is a nonzero subset of the five classes**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `each_co_signer_lives_in_its_own_record` (scope 0) (LiteSVM), `program-libs/interface/src/instructions/cosign.rs` `a_record_scope_is_a_nonzero_subset_of_the_five_classes` (unit)
  - Kind: precondition
  - Statement: every `SetCoSigner` whose scope is 0 or sets a bit above bit 4 returns Err before every account check.
  - Location: `program-libs/interface/src/instructions/instruction.rs:116-123` (`fn SetCoSignerRecordIxData::check`), `program-libs/interface/src/accounts/state.rs:597-602` (`fn CoSignScope::new`)
  - Error: `CustomRingError::InvalidCoSignerScope = 8145`
  - Severity: Medium
  - Suggested test: negative at 0 and 32, harness: LiteSVM

- [x] **INV-SET-COSIGNER-05: the signer is nonzero**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `each_co_signer_lives_in_its_own_record` (LiteSVM)
  - Kind: precondition
  - Statement: every `SetCoSigner` whose signer is the zero address returns Err.
  - Location: `program-libs/interface/src/instructions/instruction.rs:119-121` (`fn SetCoSignerRecordIxData::check`)
  - Error: `CustomRingError::InvalidCoSigner = 8146`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM

### Create

- [ ] **INV-SET-COSIGNER-06: a new record sits at its signer's canonical PDA**
  - Partial coverage: `program-tests/custom-ring/tests/rules/operator_rules.rs` `each_co_signer_lives_in_its_own_record` (LiteSVM) reads the record at the PDA. No test sends a record account at another address.
  - Kind: precondition
  - Statement: every `SetCoSigner` that creates a record at an address other than the canonical PDA of `[b"co_signer", signer]` returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_cosigner.rs:58-62` (`fn process_set_cosigner_ix`), `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs:55-63` (`fn Pda::create`)
  - Error: `CustomRingError::InvalidCoSignerRecord = 8260`
  - Severity: High (one signer must hold one record)
  - Suggested test: negative, harness: LiteSVM

- [x] **INV-SET-COSIGNER-07: a new record holds the signer, the scope and the canonical bump**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `each_co_signer_lives_in_its_own_record` (LiteSVM)
  - Kind: postcondition
  - Statement: after a `SetCoSigner` that creates a record, the record is exactly `{ discriminator: 14, signer, scope, bump }` with the canonical bump, sized exactly `CoSigner::SIZE` (35) and owned by the program.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_cosigner.rs:57-69` (`fn process_set_cosigner_ix`)
  - Severity: High
  - Suggested test: positive, harness: LiteSVM

- [x] **INV-SET-COSIGNER-08: a new record raises the count of each optional class in its scope by one**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `each_co_signer_lives_in_its_own_record` (covered classes after create) (LiteSVM), `program-libs/interface/src/instructions/cosign.rs` `a_record_scope_is_a_nonzero_subset_of_the_five_classes` (unit)
  - Kind: postcondition
  - Statement: after a `SetCoSigner` that creates a record, the ring's count of every optional class in the scope increases by exactly 1, and every other count is unchanged.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_cosigner.rs:57` (`fn process_set_cosigner_ix`), `program-libs/interface/src/accounts/state.rs:659-678` (`fn CoSignerCounts::added`, `fn step`)
  - Severity: High (the count switches a class's demand on)
  - Suggested test: positive per scope, harness: LiteSVM

### Rescope

- [x] **INV-SET-COSIGNER-09: a rescope keeps the record's signer**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `a_co_signer_write_over_an_occupied_record_needs_that_signer_s_record` (a program-owned record of another signer at the signer's PDA) (LiteSVM)
  - Kind: precondition
  - Statement: every `SetCoSigner` over an existing record whose stored `signer` differs from the instruction's signer returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_cosigner.rs:46-50` (`fn process_set_cosigner_ix`)
  - Error: `CustomRingError::InvalidCoSignerRecord = 8260`
  - Severity: High
  - Suggested test: negative, harness: LiteSVM

- [x] **INV-SET-COSIGNER-10: a rescope moves the counts from the old scope to the new one**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `each_co_signer_lives_in_its_own_record` (scope 4 then 5) (LiteSVM)
  - Kind: postcondition
  - Statement: after a `SetCoSigner` over an existing record, the record's scope is exactly the new scope, and the ring's counts are exactly the counts before with the old scope's classes removed and the new scope's classes added.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_cosigner.rs:51-54` (`fn process_set_cosigner_ix`)
  - Severity: High
  - Suggested test: positive per scope pair, harness: LiteSVM

### Counts

- [ ] **INV-SET-COSIGNER-11: a class count stays in the u8 range**
  - Partial coverage: `program-libs/interface/src/instructions/cosign.rs` `a_record_scope_is_a_nonzero_subset_of_the_five_classes` (unit) covers the underflow. No on-chain test reaches 255 records of one class.
  - Kind: precondition
  - Statement: every `SetCoSigner` that would raise a class count above 255 returns Err.
  - Location: `program-libs/interface/src/accounts/state.rs:667-678` (`fn CoSignerCounts::step`)
  - Error: `CustomRingError::CoSignerCountOutOfRange = 8263`
  - Severity: Medium
  - Suggested test: negative at 256 records, harness: unit

- [x] **INV-SET-COSIGNER-12: an approval-only scope moves no count**
  - Covered by: `program-libs/interface/src/instructions/cosign.rs` `a_record_scope_is_a_nonzero_subset_of_the_five_classes` (unit), `program-tests/custom-ring/tests/rules/transact_rules.rs` `separate_approvers_settle_a_transaction_past_both_thresholds` (LiteSVM)
  - Kind: state
  - Statement: for every record whose scope holds only approval classes, the ring's three counts are the same with and without the record.
  - Location: `program-libs/interface/src/accounts/state.rs:643-647, 667-678` (`CoSignerCounts::CLASSES`, `fn step`)
  - Severity: Medium
  - Suggested test: positive, harness: unit

### Rollback and Frame

- [ ] **INV-SET-COSIGNER-13: a write changes the ring counts and the one record alone**
  - Not covered.
  - Kind: frame
  - Statement: after every `SetCoSigner`, every ring account field other than `co_signers` is unchanged, every other record is unchanged, and only the payer and a created record move lamports.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_cosigner.rs:24-70` (`fn process_set_cosigner_ix`)
  - Severity: Medium
  - Suggested test: positive with an account snapshot, harness: LiteSVM

## ClearCoSigner

### Authorization and Accounts

- [x] **INV-CLEAR-COSIGNER-01: only the co-signer admin clears a record**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `each_role_instruction_refuses_every_other_role_and_the_upgrade_authority` (LiteSVM)
  - Kind: precondition
  - Statement: every `ClearCoSigner` whose first account is not a signer holding the `CosignAdmin` role returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_cosigner.rs:80, 86-91` (`fn process_clear_cosigner_ix`)
  - Error: `CustomRingError::UnauthorizedAuthority = 8107`
  - Severity: Critical (a clear drops an optional class's demand)
  - Suggested test: negative per other role, harness: LiteSVM

- [ ] **INV-CLEAR-COSIGNER-02: the cleared account is a co-signer record**
  - Not covered.
  - Kind: precondition
  - Statement: every `ClearCoSigner` whose third account is not a program `CoSigner` returns Err and closes nothing.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_cosigner.rs:92-94` (`fn process_clear_cosigner_ix`)
  - Error: `CustomRingError::InvalidCoSignerRecord = 8260`
  - Severity: High (a clear of the ring account would close it)
  - Suggested test: negative with the ring account in the record slot, harness: LiteSVM

- [ ] **INV-CLEAR-COSIGNER-03: the rent goes to an account other than the record**
  - Not covered.
  - Kind: precondition
  - Statement: every `ClearCoSigner` whose rent recipient is the record returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_cosigner.rs:97` (`fn process_clear_cosigner_ix`), `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs:87-102` (`fn close_into`)
  - Error: `CustomRingError::InvalidCoSignerRecord = 8260`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM

- [ ] **INV-CLEAR-COSIGNER-04: a clear takes no data**
  - Not covered.
  - Kind: precondition
  - Statement: every `ClearCoSigner` with a data byte after the tag returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_cosigner.rs:78` (`fn process_clear_cosigner_ix`)
  - Error: `CustomRingError::InvalidInstructionData = 8100`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM

### Success and Frame

- [x] **INV-CLEAR-COSIGNER-05: a clear drops the count of each optional class in the record's scope by one**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `each_co_signer_lives_in_its_own_record` (covered classes `None` after the clear) (LiteSVM)
  - Kind: postcondition
  - Statement: after a successful `ClearCoSigner`, the ring's count of every optional class in the record's scope decreases by exactly 1, and every other count is unchanged.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_cosigner.rs:92-95` (`fn process_clear_cosigner_ix`)
  - Severity: High
  - Suggested test: positive per scope, harness: LiteSVM

- [ ] **INV-CLEAR-COSIGNER-06: the record closes into its rent recipient**
  - Not covered. No test reads the record or the recipient's lamports after a clear.
  - Kind: postcondition
  - Statement: after a successful `ClearCoSigner`, the record holds 0 lamports and no data, and the rent recipient's lamports increase by exactly the record's lamports before.
  - Location: `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs:87-102` (`fn close_into`)
  - Severity: Medium
  - Suggested test: positive with lamport deltas, harness: LiteSVM

- [ ] **INV-CLEAR-COSIGNER-07: a cleared signer covers no class**
  - Not covered.
  - Kind: postcondition
  - Statement: after a successful `ClearCoSigner`, every transfer pair that names the cleared record's address returns Err.
  - Location: `programs/custom-ring/src/instructions/accounts/loader.rs:354-355` (`fn CoSignerSlot::verify`)
  - Error: `CustomRingError::InvalidCoSignerRecord = 8260`
  - Severity: High
  - Suggested test: positive then negative, harness: LiteSVM

- [ ] **INV-CLEAR-COSIGNER-08: a clear changes the counts, the record and the recipient alone**
  - Not covered.
  - Kind: frame
  - Statement: after every `ClearCoSigner`, every ring account field other than `co_signers` is unchanged, and no account other than the ring, the record and the rent recipient changes.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_cosigner.rs:73-98` (`fn process_clear_cosigner_ix`)
  - Severity: Medium
  - Suggested test: positive with an account snapshot, harness: LiteSVM
