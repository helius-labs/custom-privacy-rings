# Configuration Invariants

Covers `Initialize` (tag 53), `UpdatePolicy` (tag 54), `UpdateAuthority` (tag 6), `SetPaused` (tag 11) and `SetDelegate` (tag 64).

The role check every role-gated instruction shares (`RoleHolder`), the policy table and source rules `Initialize` and `UpdatePolicy` share (`TableBinding`, `set_policy`, `ProposedSources`), the salted source pairing, the role key rules (`RingRoles::check`) and the exact wincode parse live in `cross-cutting.md`.

## Initialize

### Authorization

- [x] **INV-INIT-01: only the program's upgrade authority creates the ring**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `initialize_creates_the_ring_and_spp_registration_under_the_upgrade_authority` (a stranger), `the_upgrade_authority_comes_from_the_ring_programs_own_program_data` (a stand-in program and its program data) (LiteSVM)
  - Kind: precondition
  - Statement: `Initialize` succeeds only when the `program` account is the invoked program id owned by the upgradeable loader, `program_data` is the address that program names and is owned by the same loader, its upgrade authority is set and nonzero, and the signing `authority` is exactly that key.
  - Location: `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:64-70` (`fn process_initialize_ix`), `programs/custom-ring/src/instructions/accounts/loader.rs:466-497` (`fn UpgradeAuthorityCheck::verify`), `loader.rs:501-510` (`fn loader_state`)
  - Error: `CustomRingError::UnauthorizedInitializer = 8114`
  - Severity: Critical (a front-runner would pick the roles and the auditor key)
  - Suggested test: negative per account and per loader state, harness: LiteSVM

- [ ] **INV-INIT-02: an immutable or non-loader-v3 deployment never initializes**
  - Not covered. No test sets the upgrade authority to none or deploys under loader-v4.
  - Kind: reachability
  - Statement: for every deployment whose program data holds no upgrade authority, and for every program not owned by the upgradeable loader, `Initialize` returns Err.
  - Location: `programs/custom-ring/src/instructions/accounts/loader.rs:468-490` (`fn UpgradeAuthorityCheck::verify`)
  - Error: `CustomRingError::UnauthorizedInitializer = 8114`
  - Severity: Medium
  - Suggested test: negative with a frozen authority, harness: LiteSVM

- [ ] **INV-INIT-03: the payer and the authority sign**
  - Partial coverage: every refusal test signs both. No test sends an unsigned payer or authority.
  - Kind: precondition
  - Statement: every `Initialize` whose payer is not a writable signer, or whose authority is not a signer, returns Err before the upgrade authority check.
  - Location: `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:49-50` (`fn process_initialize_ix`)
  - Error: `AccountError::InvalidSigner = 20009` / `AccountError::AccountNotMutable = 20002`
  - Severity: High
  - Suggested test: negative per account, harness: LiteSVM

### Account Constraints

- [x] **INV-INIT-04: the fourth account is the system program**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `initialize_takes_its_canonical_ring_address_once_beside_the_system_program` (LiteSVM)
  - Kind: precondition
  - Statement: every `Initialize` whose `system_program` account is not the system program id returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:63` (`fn process_initialize_ix`), `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs:37-42` (`fn check_system_program`)
  - Error: `CustomRingError::InvalidSystemProgram = 8111`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM

- [x] **INV-INIT-05: the ring account sits at its one canonical PDA and is new**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `initialize_takes_its_canonical_ring_address_once_beside_the_system_program` (another address, a second call) (LiteSVM)
  - Kind: precondition
  - Statement: every `Initialize` whose ring account differs from the canonical PDA of `[b"policy"]` returns `InvalidRingPolicy`, and every one whose ring account holds data returns `RingPolicyAlreadyExists`.
  - Location: `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:78-82` (`fn process_initialize_ix`), `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs:55-83` (`fn Pda::create`), `programs/custom-ring/src/state.rs:10-23` (`fn Init::init`)
  - Error: `CustomRingError::InvalidRingPolicy = 8300` / `CustomRingError::RingPolicyAlreadyExists = 8301`
  - Severity: Critical (one ring per deployment)
  - Suggested test: negative both legs, harness: LiteSVM

- [ ] **INV-INIT-06: the SPP program account is the executable SPP**
  - Not covered on `Initialize`.
  - Kind: precondition
  - Statement: every `Initialize` whose `spp_program` account is not the executable SPP program id returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:71` (`fn process_initialize_ix`)
  - Error: `CustomRingError::InvalidShieldedPoolProgram = 8103`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM

- [ ] **INV-INIT-07: the `ring_auth` account is the ring PDA**
  - Not covered on `Initialize`.
  - Kind: precondition
  - Statement: every `Initialize` whose `ring_auth` account differs from the PDA of `[RING_AUTH_PDA_SEED]` under the program id returns Err before either CPI.
  - Location: `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:134` (`fn process_initialize_ix`), `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs:119-124` (`fn RingAuth::check_account`)
  - Error: `CustomRingError::MissingRingAuth = 8104`
  - Severity: High
  - Suggested test: negative, harness: LiteSVM

### Instruction Data Validation

- [x] **INV-INIT-08: the auditor key is tagged, canonical and of large order**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `initialize_creates_the_ring_and_spp_registration_under_the_upgrade_authority` (low order) (LiteSVM), `program-libs/interface/src/proof/auditor_key.rs` `auditor_keys_must_be_tagged_canonical_and_of_large_order` (unit)
  - Kind: precondition
  - Statement: every `Initialize` whose auditor key is not `0x04 || u` with `u` a canonical X25519 coordinate outside the five low order points returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:73-75` (`fn process_initialize_ix`), `program-libs/interface/src/proof/auditor_key.rs:39-46` (`fn is_auditor_key`)
  - Error: `CustomRingError::InvalidAuditorPubkey = 8108`
  - Severity: Critical (no proof can seal to a low order key, the ring would lock for good)
  - Suggested test: negative per low order point and a noncanonical `u`, harness: unit and LiteSVM

- [x] **INV-INIT-09: the ring kind is one of four**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `each_ring_kind_fixes_its_policy_flag_and_mode` (kind byte 4) (LiteSVM)
  - Kind: precondition
  - Statement: every `Initialize` whose kind byte is not 0 to 3 returns Err.
  - Location: `program-libs/interface/src/instructions/instruction.rs:63-74` (`enum RingKind`), `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs:24-28` (`fn parse`)
  - Error: `CustomRingError::InvalidInstructionData = 8100`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM

- [x] **INV-INIT-10: a policy kind needs a policy and an audit-only kind takes no table**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `a_policy_ring_initializes_only_with_its_policy`, `only_a_policy_ring_needs_a_records_registry`, `policy_writes_need_a_policy_ring_and_a_foreign_curator` (LiteSVM)
  - Kind: precondition
  - Statement: every `Initialize` of kind `Policy` or `AnonymousPolicy` without a policy table returns `PolicyNotSet`, and every one of kind `AuditOnly` or `Anonymous` whose table holds a source or a rule returns `PolicyOnAuditOnlyRing`.
  - Location: `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:85-95` (`fn process_initialize_ix`), `initialize.rs:174-183` (`fn PolicyCreation::records`)
  - Error: `CustomRingError::PolicyNotSet = 8217` / `CustomRingError::PolicyOnAuditOnlyRing = 8142`
  - Severity: High
  - Suggested test: negative per kind, harness: LiteSVM

- [ ] **INV-INIT-11: a policy table names a records registry**
  - Not covered. No test sends a policy table with no trailing account.
  - Kind: precondition
  - Statement: every `Initialize` with a policy table and no account after `storage_program` returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:174-177` (`fn PolicyCreation::records`)
  - Error: `ProgramError::NotEnoughAccountKeys`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM

- [x] **INV-INIT-12: the records registry carries this ring's id**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `initialize_refuses_records_of_another_ring` (another ring id and the zero id, three kinds) (LiteSVM)
  - Kind: precondition
  - Statement: every `Initialize` whose records registry's `ring_id` is not exactly `hash_bytes(program_id)` returns Err and creates no ring account.
  - Location: `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:125-132` (`fn process_initialize_ix`), `programs/custom-ring/src/instructions/accounts/loader.rs:135-145` (`fn load_records_registry`)
  - Error: `CustomRingError::RecordsRegistryOtherRing = 8266`
  - Severity: Critical (a shared own registry would fail the second ring for an enrolled owner)
  - Suggested test: negative per kind, harness: LiteSVM

### Success Postconditions

- [x] **INV-INIT-13: the ring account holds the requested roles, key and pins**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `initialize_creates_the_ring_and_spp_registration_under_the_upgrade_authority`, `initialize_refuses_a_zero_role_key_and_takes_a_shared_one`, `the_ring_records_the_slot_it_was_created_in` (LiteSVM)
  - Kind: postcondition
  - Statement: after a successful `Initialize`, the ring account holds exactly the requested roles and auditor key, `created_slot` equal to the clock slot, `records` equal to the first trailing account or zero without a policy table, `tracking` equal to the `tracking_registry` account, no delegate and every co-signer count 0.
  - Location: `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:96-110` (`fn process_initialize_ix`), `program-libs/interface/src/accounts/state.rs:199-213` (`fn NewRing::account`)
  - Severity: High
  - Suggested test: positive, harness: LiteSVM

- [x] **INV-INIT-14: the kind fixes the mode and the policy flag**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `each_ring_kind_fixes_its_policy_flag_and_mode` (LiteSVM)
  - Kind: postcondition
  - Statement: after a successful `Initialize`, `mode` is exactly 2 for the anonymous kinds and 0 otherwise, and `policy_hash` is nonzero exactly for the policy kinds.
  - Location: `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:99-103, 111-123` (`fn process_initialize_ix`)
  - Severity: High
  - Suggested test: positive per kind, harness: LiteSVM

- [x] **INV-INIT-15: storage creates the tracking registry the ring pins**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `initialize_creates_the_ring_and_spp_registration_under_the_upgrade_authority`, `initialize_refuses_a_tracking_account_off_its_seeds` (LiteSVM)
  - Kind: postcondition
  - Statement: after a successful `Initialize`, the pinned tracking registry is the storage PDA of `[tracking_registry, ring_auth, hash_bytes(program_id)]`, its authority is `ring_auth`, its ring id is `hash_bytes(program_id)` and its auditor key is the ring's. A `tracking_registry` account at another address makes the storage CPI fail and the ring account is not created.
  - Location: `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:135-149` (`fn process_initialize_ix`), `programs/storage/src/loader.rs:66-78` (`fn TrackingRegistryInit::init`)
  - Error: `StorageError::InvalidTrackingRegistry = 9022`
  - Severity: Critical (a forged registry would prove any owner absent)
  - Suggested test: negative with an off-seed account, harness: LiteSVM

- [x] **INV-INIT-16: SPP registers the ring with `ring_auth` as its authority**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `initialize_creates_the_ring_and_spp_registration_under_the_upgrade_authority` (LiteSVM), `program-tests/custom-ring/tests/ring/transfers.rs` `auditor_sees_every_ring_transfer` (localnet)
  - Kind: postcondition
  - Statement: after a successful `Initialize`, SPP's `RingConfig` at `ring_auth` names exactly the invoked program id and the `ring_auth` address as its authority.
  - Location: `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:151-162` (`fn process_initialize_ix`)
  - Severity: Critical (SPP admin actions on the ring then pass only through the program)
  - Suggested test: positive, harness: LiteSVM

### Rollback and Frame

- [x] **INV-INIT-17: a refused `Initialize` leaves no ring account**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `initialize_creates_the_ring_and_spp_registration_under_the_upgrade_authority`, `initialize_refuses_a_tracking_account_off_its_seeds`, `initialize_refuses_a_zero_role_key_and_takes_a_shared_one`, `initialize_refuses_each_table_form` (ring PDA lamports stay 0) (LiteSVM), `program-tests/custom-ring/tests/rules/salted_source_rules.rs` `initialize_stores_a_salted_source_only_when_it_pairs_with_the_ring` (LiteSVM)
  - Kind: rollback
  - Statement: when `Initialize` returns Err at any stage, including a refusal inside the storage or SPP CPI after the ring account was written, the ring PDA, the tracking registry and `RingConfig` hold the same lamports and data as before.
  - Location: `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:78-163` (`fn process_initialize_ix`, the account is written at 133 ahead of both CPIs)
  - Severity: Critical
  - Suggested test: negative per stage with a full account snapshot, harness: LiteSVM

- [ ] **INV-INIT-18: `Initialize` writes only its own creations**
  - Not covered.
  - Kind: frame
  - Statement: after a successful `Initialize`, every account other than the payer, the ring PDA, the tracking registry and `ring_auth` is unchanged, and the trailing registries are unchanged.
  - Location: `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:42-163` (`fn process_initialize_ix`)
  - Severity: Medium
  - Suggested test: positive with an account snapshot, harness: LiteSVM

## UpdatePolicy

### Authorization

- [x] **INV-UPDATE-POLICY-01: only the policy authority writes the policy**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `each_role_instruction_refuses_every_other_role_and_the_upgrade_authority` (rules, source and records writes) (LiteSVM)
  - Kind: precondition
  - Statement: every `UpdatePolicy` whose first account is not a signer holding the `Policy` role returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/policy/update_policy.rs:42-52` (`fn process_update_policy_ix`)
  - Error: `CustomRingError::UnauthorizedAuthority = 8107`
  - Severity: Critical
  - Suggested test: negative per other role, harness: LiteSVM

### Instruction Data Validation

- [ ] **INV-UPDATE-POLICY-02: the write names one of four variants**
  - Not covered. No test sends a variant byte above 3.
  - Kind: precondition
  - Statement: every `UpdatePolicy` whose variant byte is not 0 to 3, or whose data has a trailing byte, returns Err.
  - Location: `program-libs/interface/src/instructions/instruction.rs:166-180` (`enum UpdatePolicyIxData`), `programs/custom-ring/src/instructions/configuration/policy/update_policy.rs:40` (`fn process_update_policy_ix`)
  - Error: `CustomRingError::InvalidInstructionData = 8100`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM

### Write Rules

- [x] **INV-UPDATE-POLICY-03: an audit-only ring takes no table and no ring salt**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `policy_writes_need_a_policy_ring_and_a_foreign_curator` (LiteSVM), `program-tests/custom-ring/tests/rules/salted_source_rules.rs` `the_policy_authority_commits_a_ring_salt_and_an_audit_only_ring_none` (LiteSVM)
  - Kind: precondition
  - Statement: every `Rules` or `RingListTag` write on a ring whose `policy_hash` is zero returns Err and leaves the ring account unchanged.
  - Location: `programs/custom-ring/src/instructions/configuration/policy/update_policy.rs:80, 145` (`fn replace_rules`, `fn commit_ring_salt`), `program-libs/interface/src/accounts/state.rs:252-255` (`fn policy_hash`)
  - Error: `CustomRingError::PolicyNotSet = 8217`
  - Severity: High
  - Suggested test: negative, harness: LiteSVM

- [x] **INV-UPDATE-POLICY-04: a source write names a known list**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `a_source_write_names_a_known_list` (list ids 0 and 255) (LiteSVM)
  - Kind: precondition
  - Statement: every `Source` write whose list id is not a `ListId` returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/policy/update_policy.rs:99-100` (`fn move_source`)
  - Error: `CustomRingError::InvalidListId = 8126`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM

- [ ] **INV-UPDATE-POLICY-05: a source moves only for a list the rules reference, to the records or one nonzero curator**
  - Partial coverage: `program-tests/custom-ring/tests/rules/operator_rules.rs` `a_nonzero_source_refuses_the_default_curator_address` covers a zero curator, and `a_source_move_names_no_curator_past_the_first` covers source bytes 2 and 255 (LiteSVM). No test moves the source of an unreferenced list or sends source 1 with no trailing account.
  - Kind: precondition
  - Statement: every `Source` write returns Err when its source byte is not 0 or 1, when source 1 has no trailing account or a zero first trailing account, or when the list's slot has no source.
  - Location: `programs/custom-ring/src/instructions/configuration/policy/update_policy.rs:101-113` (`fn move_source`), `programs/custom-ring/src/instructions/verification/policy_binding.rs:71-86` (`fn source_registry`)
  - Error: `CustomRingError::InvalidSource = 8134`
  - Severity: High
  - Suggested test: negative per case, harness: LiteSVM

- [x] **INV-UPDATE-POLICY-06: a records rebind names a registry of this ring**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `the_records_write_refuses_a_registry_of_another_ring`, `update_policy_binds_records_late_on_an_audit_only_ring` (LiteSVM), `program-tests/custom-ring/tests/rules/storage_root_rules.rs` `update_policy_rebinds_the_records_to_a_registry_of_any_authority` (LiteSVM)
  - Kind: precondition
  - Statement: every `Records` write returns `InvalidStorageRegistry` when it has no trailing account or the first one is not a storage-owned `StorageRegistry`, and `RecordsRegistryOtherRing` when that registry's ring id is not `hash_bytes(program_id)`.
  - Location: `programs/custom-ring/src/instructions/configuration/policy/update_policy.rs:132-140` (`fn rebind_records`), `update_policy.rs:164-174` (`fn commit`)
  - Error: `CustomRingError::InvalidStorageRegistry = 8302` / `CustomRingError::RecordsRegistryOtherRing = 8266`
  - Severity: Critical (the records hold the thresholds and the own lists)
  - Suggested test: negative per case, harness: LiteSVM

- [x] **INV-UPDATE-POLICY-07: no write clears the records**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `the_records_write_refuses_a_registry_of_another_ring` (the zero address) (LiteSVM)
  - Kind: reachability
  - Statement: no `UpdatePolicy` sets `records` to the zero address. A `Records` write naming the zero address returns Err, and no other variant writes `records`.
  - Location: `programs/custom-ring/src/instructions/configuration/policy/update_policy.rs:132-140, 164-174` (`fn rebind_records`, `fn commit`), `program-libs/interface/src/accounts/state.rs:297-301` (`fn rebind_records`)
  - Error: `CustomRingError::InvalidStorageRegistry = 8302`
  - Severity: High (a ring with records would drop its thresholds)
  - Suggested test: negative, harness: LiteSVM

- [x] **INV-UPDATE-POLICY-08: a ring salt off the field is refused**
  - Covered by: `program-tests/custom-ring/tests/rules/salted_source_rules.rs` `the_policy_authority_commits_a_ring_salt_and_an_audit_only_ring_none` (LiteSVM)
  - Kind: precondition
  - Statement: every `RingListTag` write whose tag is not a BN254 field value returns Err and leaves the ring account unchanged.
  - Location: `program-libs/interface/src/accounts/state.rs:280-295` (`fn set_ring_list_tag`), `program-libs/policy/src/rule_table/mod.rs:685-697` (`fn EncodedRuleTable::hash`)
  - Error: `CustomRingError::HashingFailed = 8102`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM

### Success Postconditions

- [ ] **INV-UPDATE-POLICY-09: every write counts one generation at its slot**
  - Partial coverage: `program-tests/custom-ring/tests/rules/operator_rules.rs` `policy_writes_need_a_policy_ring_and_a_foreign_curator` (+2) and `program-tests/custom-ring/tests/rules/storage_root_rules.rs` `update_policy_rebinds_the_records_to_a_registry_of_any_authority` (+1) (LiteSVM) assert the count. No test asserts `generation_slot` or the count after a `RingListTag` write.
  - Kind: postcondition
  - Statement: after every successful `UpdatePolicy`, `generation` is exactly its value before plus 1 and `generation_slot` is exactly the clock slot.
  - Location: `program-libs/interface/src/accounts/state.rs:303-311` (`fn count_generation`), called from `set_policy` (263-277) and `rebind_records` (297-301)
  - Severity: Medium
  - Suggested test: positive per variant, harness: LiteSVM

- [x] **INV-UPDATE-POLICY-10: the generation counter refuses to wrap**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `a_table_write_decodes_and_counts_its_generation_on_the_policy` (LiteSVM)
  - Kind: precondition
  - Statement: every `UpdatePolicy` on a ring whose `generation` is `u32::MAX` returns Err.
  - Location: `program-libs/interface/src/accounts/state.rs:304-307` (`fn count_generation`)
  - Error: `CustomRingError::PolicyGenerationOverflow = 8141`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM

- [x] **INV-UPDATE-POLICY-11: a table write repins the policy hash and a records write keeps it**
  - Covered by: `program-tests/custom-ring/tests/rules/storage_root_rules.rs` `update_policy_rebinds_the_records_to_a_registry_of_any_authority` (records new, hash kept) (LiteSVM), `program-tests/custom-ring/tests/rules/salted_source_rules.rs` `the_policy_authority_commits_a_ring_salt_and_an_audit_only_ring_none` (hash moves with the tag, returns with the zero tag) (LiteSVM)
  - Kind: postcondition
  - Statement: after a successful `Rules`, `Source` or `RingListTag` write, `policy_hash` is exactly the table hash of the stored rules, sources and ring list tag. After a successful `Records` write, `policy_hash`, `rules` and `sources` are unchanged and `records` is exactly the first trailing account.
  - Location: `program-libs/interface/src/accounts/state.rs:263-301` (`fn set_policy`, `fn set_ring_list_tag`, `fn rebind_records`)
  - Severity: High
  - Suggested test: positive per variant, harness: LiteSVM

- [x] **INV-UPDATE-POLICY-12: the next proof reads the written state**
  - Covered by: `program-tests/custom-ring/tests/rules/storage_root_rules.rs` `update_policy_rebinds_the_records_to_a_registry_of_any_authority` (a transfer naming the old records meets `UnboundRegistry`) (LiteSVM)
  - Kind: postcondition
  - Statement: after a successful `UpdatePolicy`, every transfer that names the replaced records registry returns Err, and every policy transfer binds the new `policy_hash`.
  - Location: `programs/custom-ring/src/instructions/configuration/policy/update_policy.rs:59-65` (`fn process_update_policy_ix`), `programs/custom-ring/src/instructions/accounts/loader.rs:176-210` (`fn RegistrySlots::load`)
  - Error: `CustomRingError::UnboundRegistry = 8480`
  - Severity: High
  - Suggested test: positive then negative, harness: LiteSVM

### Rollback and Frame

- [x] **INV-UPDATE-POLICY-13: a refused write leaves the stored account unchanged**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `a_table_write_refuses_each_table_form` (LiteSVM), every `refused_unchanged` case in `program-tests/custom-ring/tests/rules/salted_source_rules.rs` (LiteSVM)
  - Kind: rollback
  - Statement: when `UpdatePolicy` returns Err, every byte of the ring account equals its value before, including when the refusal comes from a registry check after the proposed account was built.
  - Location: `programs/custom-ring/src/instructions/configuration/policy/update_policy.rs:154-177` (`fn PolicyWrite::commit`)
  - Severity: High
  - Suggested test: negative per refusal with a byte compare, harness: LiteSVM

- [ ] **INV-UPDATE-POLICY-14: a policy write changes the ring account alone**
  - Not covered.
  - Kind: frame
  - Statement: after every `UpdatePolicy`, every trailing registry and every other account is unchanged in data and lamports.
  - Location: `programs/custom-ring/src/instructions/configuration/policy/update_policy.rs:35-66` (`fn process_update_policy_ix`)
  - Severity: Medium
  - Suggested test: positive with an account snapshot, harness: LiteSVM

- [ ] **INV-UPDATE-POLICY-15: a policy write leaves the roles, the delegate, the mode and the pins unchanged**
  - Partial coverage: `program-tests/custom-ring/tests/rules/operator_rules.rs` `update_policy_binds_records_late_on_an_audit_only_ring` (LiteSVM) asserts the audit-only ring keeps a zero policy hash. No test compares `roles`, `delegate`, `mode`, `auditor_pubkey`, `tracking` and `co_signers` across a policy write.
  - Kind: frame
  - Statement: after every successful `UpdatePolicy`, `roles`, `delegate`, `mode`, `auditor_pubkey`, `tracking`, `created_slot` and `co_signers` are unchanged, and an audit-only ring keeps a zero `policy_hash`.
  - Location: `program-libs/interface/src/accounts/state.rs:263-301` (`fn set_policy`, `fn set_ring_list_tag`, `fn rebind_records`)
  - Severity: High
  - Suggested test: positive with a field compare, harness: LiteSVM

## UpdateAuthority

### Authorization

- [x] **INV-UPDATE-AUTH-01: only the admin rotates a role**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `each_role_instruction_refuses_every_other_role_and_the_upgrade_authority`, `a_rotated_admin_appoints_the_delegate_and_the_old_one_cannot` (LiteSVM)
  - Kind: precondition
  - Statement: every `UpdateAuthority` whose first account is not a signer holding the `Admin` role returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/access/update_authority.rs:20-30` (`fn process_update_authority_ix`)
  - Error: `CustomRingError::UnauthorizedAuthority = 8107`
  - Severity: Critical (authority takeover)
  - Suggested test: negative per other role, harness: LiteSVM

- [x] **INV-UPDATE-AUTH-02: the new key signs**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `update_authority_needs_the_new_keys_signature` (LiteSVM)
  - Kind: precondition
  - Statement: every `UpdateAuthority` whose second account is not a signer returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/access/update_authority.rs:21` (`fn process_update_authority_ix`)
  - Error: `AccountError::InvalidSigner = 20009`
  - Severity: High (a mistyped key would strand the role)
  - Suggested test: negative, harness: LiteSVM

### Instruction Data and State Rules

- [x] **INV-UPDATE-AUTH-03: the role byte names one of five roles**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `update_authority_needs_the_new_keys_signature` (role 5) (LiteSVM)
  - Kind: precondition
  - Statement: every `UpdateAuthority` whose role byte is not 0 to 4 returns Err.
  - Location: `program-libs/interface/src/accounts/state.rs:504-517` (`enum RingRole`), `programs/custom-ring/src/instructions/configuration/access/update_authority.rs:18` (`fn process_update_authority_ix`)
  - Error: `CustomRingError::InvalidInstructionData = 8100`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM

- [ ] **INV-UPDATE-AUTH-04: no role passes to the delegate**
  - Partial coverage: `sdk-libs/client/tests/accounts.rs` `role_keys_stay_nonzero_and_apart_from_the_delegate` (library). No on-chain test rotates a role to the delegate.
  - Kind: precondition
  - Statement: every `UpdateAuthority` on a ring with a delegate whose new key equals the delegate returns Err and leaves the roles unchanged.
  - Location: `program-libs/interface/src/accounts/state.rs:243-249` (`fn rotate_role`), `state.rs:541-560` (`fn RingRoles::check`)
  - Error: `CustomRingError::DuplicateRoleKey = 8472`
  - Severity: Critical (the delegate would gain a role beside its moves)
  - Suggested test: negative per role, harness: LiteSVM

### Success, Rollback and Frame

- [x] **INV-UPDATE-AUTH-05: exactly the named role moves to the new key**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `update_authority_needs_the_new_keys_signature` (asserts every role key after each rotation) (LiteSVM)
  - Kind: postcondition
  - Statement: after a successful `UpdateAuthority`, the named role's key is exactly the new key and every other role key is unchanged. One key may hold several roles.
  - Location: `program-libs/interface/src/accounts/state.rs:243-249` (`fn rotate_role`)
  - Severity: High
  - Suggested test: positive per role, harness: LiteSVM

- [x] **INV-UPDATE-AUTH-06: the old holder loses the role in the same instruction**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `a_rotated_admin_appoints_the_delegate_and_the_old_one_cannot` (LiteSVM)
  - Kind: postcondition
  - Statement: after a successful rotation of role R, every later R-gated instruction signed by the previous holder returns Err.
  - Location: `program-libs/interface/src/accounts/state.rs:235-240` (`fn check_role`)
  - Error: `CustomRingError::UnauthorizedAuthority = 8107`
  - Severity: Critical
  - Suggested test: positive then negative, harness: LiteSVM

- [ ] **INV-UPDATE-AUTH-07: a rotation changes the roles alone**
  - Not covered.
  - Kind: frame
  - Statement: after every `UpdateAuthority`, every ring account field other than `roles` is unchanged, and no other account changes.
  - Location: `programs/custom-ring/src/instructions/configuration/access/update_authority.rs:13-33` (`fn process_update_authority_ix`)
  - Severity: Medium
  - Suggested test: positive with a field compare, harness: LiteSVM

## SetPaused

### Authorization and Accounts

- [x] **INV-SET-PAUSED-01: only the pauser pauses**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `each_role_instruction_refuses_every_other_role_and_the_upgrade_authority` (LiteSVM)
  - Kind: precondition
  - Statement: every `SetPaused` whose first account is not a signer holding the `Pauser` role returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_paused.rs:25-36` (`fn process_set_paused_ix`)
  - Error: `CustomRingError::UnauthorizedAuthority = 8107`
  - Severity: High
  - Suggested test: negative per other role, harness: LiteSVM

- [x] **INV-SET-PAUSED-02: the call reaches SPP through the ring PDA**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `only_the_pauser_pauses_through_the_ring_pda_and_the_pool` (LiteSVM)
  - Kind: precondition
  - Statement: every `SetPaused` whose `spp_program` account is not the executable SPP returns `InvalidShieldedPoolProgram`, and every one whose `ring_auth` account is not the ring PDA returns `MissingRingAuth`.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_paused.rs:37-38` (`fn process_set_paused_ix`)
  - Error: `CustomRingError::InvalidShieldedPoolProgram = 8103` / `CustomRingError::MissingRingAuth = 8104`
  - Severity: Medium
  - Suggested test: negative both legs, harness: LiteSVM

- [ ] **INV-SET-PAUSED-03: the data is one boolean byte**
  - Not covered.
  - Kind: precondition
  - Statement: every `SetPaused` whose data after the tag is not exactly one byte of value 0 or 1 returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_paused.rs:22` (`fn process_set_paused_ix`), `program-libs/interface/src/instructions/instruction.rs:102-105` (`struct SetPausedIxData`)
  - Error: `CustomRingError::InvalidInstructionData = 8100`
  - Severity: Medium
  - Suggested test: negative with byte 2 and with two bytes, harness: LiteSVM

### Success and Frame

- [x] **INV-SET-PAUSED-04: SPP's ring config takes the requested pause flag**
  - Covered by: `program-tests/custom-ring/tests/ring/transfers.rs` `auditor_sees_every_ring_transfer` (paused by the pauser, a deposit then meets SPP's `RingPaused`, resume reopens it) (localnet), `sdk-libs/client/tests/instruction_builders.rs` `pause_and_resume_reach_spp_through_the_ring_auth` (library)
  - Kind: postcondition
  - Statement: after a successful `SetPaused`, SPP's `RingConfig.paused` is exactly the requested value, set by one SPP `UpdateRingConfig` call whose authority and ring config are both `ring_auth`.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_paused.rs:39-41` (`fn process_set_paused_ix`)
  - Severity: High
  - Suggested test: positive both values, harness: localnet

- [x] **INV-SET-PAUSED-05: no human key writes the ring config past the program**
  - Covered by: `program-tests/custom-ring/tests/ring/transfers.rs` `auditor_sees_every_ring_transfer` (a direct SPP `UpdateRingConfig` meets `UnauthorizedCaller`) (localnet)
  - Kind: reachability
  - Statement: for every key other than `ring_auth`, a direct SPP `UpdateRingConfig` of the ring returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/setup/initialize.rs:155-159` (`fn process_initialize_ix`, the registered authority)
  - Severity: High
  - Suggested test: negative, harness: localnet

- [ ] **INV-SET-PAUSED-06: a pause changes no ring program account**
  - Not covered.
  - Kind: frame
  - Statement: after every `SetPaused`, the ring account is unchanged, and the only account the call changes is SPP's `RingConfig`.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_paused.rs:17-42` (`fn process_set_paused_ix`)
  - Severity: Medium
  - Suggested test: positive with an account snapshot, harness: localnet

## SetDelegate

### Authorization

- [x] **INV-SET-DELEGATE-01: only the admin appoints the delegate**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `each_role_instruction_refuses_every_other_role_and_the_upgrade_authority`, `a_rotated_admin_appoints_the_delegate_and_the_old_one_cannot` (LiteSVM)
  - Kind: precondition
  - Statement: every `SetDelegate` whose first account is not a signer holding the `Admin` role returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_delegate.rs:28-38` (`fn process_set_delegate_ix`)
  - Error: `CustomRingError::UnauthorizedAuthority = 8107`
  - Severity: Critical (the delegate moves every note)
  - Suggested test: negative per other role, harness: LiteSVM

- [x] **INV-SET-DELEGATE-02: the delegate signs its appointment and no data names it**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `the_delegate_signs_its_appointment_and_no_data_names_it` (LiteSVM)
  - Kind: precondition
  - Statement: every `SetDelegate` whose third account is unsigned returns `InvalidSigner`, and every one with a data byte after the tag returns `InvalidInstructionData`.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_delegate.rs:26, 30` (`fn process_set_delegate_ix`)
  - Error: `AccountError::InvalidSigner = 20009` / `CustomRingError::InvalidInstructionData = 8100`
  - Severity: High
  - Suggested test: negative both legs, harness: LiteSVM

### State Rules

- [x] **INV-SET-DELEGATE-03: the delegate is set once**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `the_delegate_is_permanent_and_any_ring_takes_one` (LiteSVM)
  - Kind: precondition
  - Statement: every `SetDelegate` on a ring whose `delegate` is nonzero returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_delegate.rs:39-41` (`fn process_set_delegate_ix`)
  - Error: `CustomRingError::DelegateAlreadySet = 8153`
  - Severity: Critical
  - Suggested test: negative, harness: LiteSVM

- [x] **INV-SET-DELEGATE-04: the delegate holds no role**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `the_delegate_is_permanent_and_any_ring_takes_one` (the pauser as delegate) (LiteSVM)
  - Kind: precondition
  - Statement: every `SetDelegate` whose delegate equals one of the five role keys returns Err.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_delegate.rs:43` (`fn process_set_delegate_ix`), `program-libs/interface/src/accounts/state.rs:541-560` (`fn RingRoles::check`)
  - Error: `CustomRingError::DuplicateRoleKey = 8472`
  - Severity: High
  - Suggested test: negative per role, harness: LiteSVM

- [x] **INV-SET-DELEGATE-05: the delegate is permanent**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `the_delegate_is_permanent_and_any_ring_takes_one` (LiteSVM)
  - Kind: state
  - Statement: once `delegate` is nonzero, no ring instruction changes it.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_delegate.rs:39-44` (`fn process_set_delegate_ix`, the one writer of `delegate`)
  - Severity: Critical
  - Suggested test: property over every instruction after an appointment, harness: LiteSVM

### Success and Frame

- [x] **INV-SET-DELEGATE-06: the delegate is the signer and the mode stays**
  - Covered by: `program-tests/custom-ring/tests/rules/operator_rules.rs` `the_delegate_is_permanent_and_any_ring_takes_one` (every ring kind) (LiteSVM)
  - Kind: postcondition
  - Statement: after a successful `SetDelegate`, `delegate` is exactly the third account's address and `mode` is unchanged.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_delegate.rs:44` (`fn process_set_delegate_ix`)
  - Severity: High
  - Suggested test: positive per kind, harness: LiteSVM

- [ ] **INV-SET-DELEGATE-07: an appointment changes the delegate alone**
  - Not covered.
  - Kind: frame
  - Statement: after every `SetDelegate`, every ring account field other than `delegate` is unchanged, and no other account changes.
  - Location: `programs/custom-ring/src/instructions/configuration/access/set_delegate.rs:21-46` (`fn process_set_delegate_ix`)
  - Severity: Medium
  - Suggested test: positive with a field compare, harness: LiteSVM
