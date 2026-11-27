# Merge Invariants

Covers `Merge` (tag 57).

The ring program verifies the ring merge proof over the auditor record, then forwards SPP's `RING_MERGE_TRANSACT` with `ring_auth` signing. SPP's own merge proof ties every input to one owner, asset and sum. Invariants shared with `Transfer` (registry slots, the `ring_auth` slot, the forwarded payer, the SPP program check, the ring proof, live roots) live in `cross-cutting.md`.

## Merge

### Instruction Data Validation

- [x] **INV-MERGE-01: the merge data parses in one fixed order**
  - Covered by: `sdk-libs/client/tests/merge.rs` `the_frame_is_the_shared_vector`, `framing_errors_are_refused_before_or_by_the_spp_parser` (library)
  - Kind: precondition
  - Statement: every merge whose data is shorter than the tracking revision, the output ring data hash, the ring proof, the ephemeral key, the envelope, the record and the SPP body head through the compressed `proof.b` returns Err before every account check.
  - Location: `program-libs/interface/src/instructions/merge.rs:36-47` (`fn CustomRingMergeIxDataRef::from_bytes`), `merge.rs:73-79` (`fn SppMergeBody::read`)
  - Error: `CustomRingError::InvalidInstructionData = 8100`
  - Severity: Medium
  - Suggested test: negative at every section boundary, harness: library

- [x] **INV-MERGE-02: the ephemeral key carries the viewing key tag**
  - Covered by: `program-tests/custom-ring/tests/rules/merge_rules.rs` `an_ephemeral_key_off_its_tag_is_refused` (LiteSVM)
  - Kind: precondition
  - Statement: every merge whose `eph_pk[0]` is not exactly `VIEWING_PUBKEY_TAG` (4) returns Err.
  - Location: `programs/custom-ring/src/instructions/settlement/merge.rs:59-61` (`fn process_merge_ix`)
  - Error: `CustomRingError::InvalidMergeDisclosure = 8179`
  - Severity: Medium
  - Suggested test: negative per tag byte, harness: LiteSVM

- [x] **INV-MERGE-03: an SPP merge point off the curve is refused before the record pairing**
  - Covered by: `program-tests/custom-ring/tests/rules/merge_rules.rs` `a_merge_proof_point_off_the_curve_is_refused_before_the_forward` (LiteSVM), `sdk-libs/client/tests/merge.rs` `a_compressed_point_off_the_curve_is_refused` (library)
  - Kind: precondition
  - Statement: every merge whose compressed SPP `proof.b` does not decompress returns Err before the ring proof check.
  - Location: `programs/custom-ring/src/instructions/settlement/merge.rs:67` (`fn process_merge_ix`), `program-libs/interface/src/instructions/merge.rs:81-85` (`fn SppMergeBody::decompressed`)
  - Error: `CustomRingError::InvalidSppProofPoint = 8218`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM

- [ ] **INV-MERGE-04: the decompressed body parses as SPP's merge data with a first nullifier**
  - Partial coverage: `sdk-libs/client/tests/merge.rs` `framing_errors_are_refused_before_or_by_the_spp_parser` (library). No on-chain test sends a body SPP's parser refuses.
  - Kind: precondition
  - Statement: every merge whose decompressed body does not parse as `MergeTransactIxDataRef`, or parses with no nullifier, returns Err before the ring proof check.
  - Location: `programs/custom-ring/src/instructions/settlement/merge.rs:68-73` (`fn process_merge_ix`)
  - Error: `CustomRingError::InvalidInstructionData = 8100`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM

### Account Constraints

- [ ] **INV-MERGE-05: the first account is the ring account**
  - Not covered on the merge path. `program-tests/custom-ring/tests/rules/transact_rules.rs` `a_ring_account_of_another_owner_is_refused` covers the same loader on a transfer.
  - Kind: precondition
  - Statement: every merge whose first account is not the program's `RingPolicy` (owner, exact size, discriminator 15, known mode) returns Err.
  - Location: `programs/custom-ring/src/instructions/settlement/merge.rs:46` (`fn process_merge_ix`), `programs/custom-ring/src/instructions/accounts/loader.rs:38-47` (`fn load`), `loader.rs:70-80` (`fn OwnedAccount::load`)
  - Error: `CustomRingError::InvalidRingPolicy = 8300`
  - Severity: High
  - Suggested test: negative, harness: LiteSVM

- [x] **INV-MERGE-06: a merge reads the pinned tracking registry and no policy registry**
  - Covered by: `program-tests/custom-ring/tests/rules/merge_rules.rs` `a_merge_reads_only_the_pinned_tracking_registry` (LiteSVM)
  - Kind: precondition
  - Statement: every merge reads exactly one registry, the account right after the ring account, and returns Err when that address differs from the ring's `tracking` pin.
  - Location: `programs/custom-ring/src/instructions/settlement/merge.rs:48-52` (`fn process_merge_ix`), `program-libs/interface/src/accounts/state.rs:436-438` (`fn RootRead::merge`)
  - Error: `CustomRingError::UnboundRegistry = 8480`
  - Severity: Critical (a forged registry would prove any owner's row)
  - Suggested test: negative with a copy of the registry at another address, harness: LiteSVM

- [x] **INV-MERGE-07: `ring_auth` sits in SPP's merge `ring_config` slot alone**
  - Covered by: `program-tests/custom-ring/tests/rules/merge_rules.rs` `a_merge_forward_holds_the_ring_pda_and_a_signing_payer`, `a_merge_naming_the_ring_pda_past_its_slot_is_refused` (LiteSVM)
  - Kind: precondition
  - Statement: every merge whose forwarded list does not hold `ring_auth` at index `RING_MERGE_RING_CONFIG_INDEX` (2) returns `MissingRingAuth`, and every one that holds it at a second index returns `RingAuthOutOfSlot`.
  - Location: `programs/custom-ring/src/instructions/settlement/merge.rs:55-56` (`fn process_merge_ix`)
  - Error: `CustomRingError::MissingRingAuth = 8104` / `CustomRingError::RingAuthOutOfSlot = 8234`
  - Severity: Critical
  - Suggested test: negative both legs, harness: LiteSVM

- [x] **INV-MERGE-08: the merge payer signs**
  - Covered by: `program-tests/custom-ring/tests/rules/merge_rules.rs` `a_merge_forward_holds_the_ring_pda_and_a_signing_payer` (LiteSVM)
  - Kind: precondition
  - Statement: every merge whose forwarded account at `RING_MERGE_PAYER_INDEX` (3) does not sign returns Err.
  - Location: `programs/custom-ring/src/instructions/settlement/merge.rs:57` (`fn process_merge_ix`)
  - Error: `CustomRingError::MissingFundingSigner = 8173`
  - Severity: High
  - Suggested test: negative, harness: LiteSVM

- [x] **INV-MERGE-09: the forwarded list holds the SPP program**
  - Covered by: `program-tests/custom-ring/tests/rules/merge_rules.rs` `a_merge_without_the_shielded_pool_program_refuses_before_verification` (LiteSVM)
  - Kind: precondition
  - Statement: every merge whose forwarded list holds no executable account at the SPP program id returns Err before the ring proof check.
  - Location: `programs/custom-ring/src/instructions/settlement/merge.rs:58` (`fn process_merge_ix`)
  - Error: `CustomRingError::InvalidShieldedPoolProgram = 8103`
  - Severity: Medium
  - Suggested test: negative, harness: LiteSVM

### Roots

- [x] **INV-MERGE-10: the tracking root is the one the pinned registry serves**
  - Covered by: `program-tests/custom-ring/tests/rules/merge_rules.rs` `a_nonzero_merge_binds_a_served_tracking_root_on_every_ring` (LiteSVM)
  - Kind: precondition
  - Statement: the statement's `tracking_root` is exactly the root the pinned tracking registry serves at the merge's revision and the current slot, and every revision it does not serve returns Err.
  - Location: `programs/custom-ring/src/instructions/settlement/merge.rs:63-65` (`fn process_merge_ix`), `program-libs/interface/src/accounts/state.rs:465-472` (`fn LiveRoots::tracking`)
  - Error: `CustomRingError::RootNotServed = 8481`
  - Severity: Critical (the proof opens the owner's enrolled row under it)
  - Suggested test: negative with a stale revision on every ring kind, harness: LiteSVM

- [ ] **INV-MERGE-11: a merge answers to no tracking floor**
  - Not covered. No merge test pins a records registry with a floor.
  - Kind: state
  - Statement: for every merge, the tracking floor applied is exactly 0, whatever floors the ring's sources hold.
  - Location: `programs/custom-ring/src/instructions/settlement/merge.rs:48-52` (`fn process_merge_ix`), `programs/custom-ring/src/instructions/accounts/loader.rs:176-210` (`fn RegistrySlots::load`, no source loads on a `None` read)
  - Severity: Medium (a merge runs no rule, a floor would only stall it)
  - Suggested test: positive with a floor above the named revision, harness: LiteSVM

### Statement and Proof

- [ ] **INV-MERGE-12: the merge statement takes its ring values from accounts and SPP's body**
  - Partial coverage: `program-tests/custom-ring/tests/rules/merge_rules.rs` `a_ciphertext_element_past_the_field_is_refused` (LiteSVM) shows the ciphertext enters the statement. No test pins the element values or the order.
  - Kind: postcondition
  - Statement: the merge statement is exactly the chain of `MERGE_STATEMENT`, SPP's `output_utxo_hash`, SPP's first nullifier, `hash_bytes(program_id)`, the ring account's auditor key in two limbs, the ephemeral key in two limbs, `hash_bytes(envelope)`, `hash_bytes(record)`, `solana_owner_identity` of the payer at index 3, the served tracking root, and SPP's output salt and three ciphertext limbs.
  - Location: `programs/custom-ring/src/instructions/settlement/merge.rs:75-90` (`fn process_merge_ix`), `program-libs/interface/src/instructions/merge.rs:110-133` (`fn MergePublicInput::elements`)
  - Error: `CustomRingError::HashingFailed = 8102` (a ciphertext limb off the field)
  - Severity: Critical (the statement alone binds the auditor record of a merge)
  - Suggested test: golden vector against `prover/circuits/merge.go`, harness: library

- [x] **INV-MERGE-13: the merge proof verifies under the merge key before `ring_auth` signs**
  - Covered by: every `reaches_verification` case in `program-tests/custom-ring/tests/rules/merge_rules.rs` (LiteSVM), `program-tests/custom-ring/tests/ring/merge.rs` `cli_merges_fragmented_custom_ring_notes` (localnet)
  - Kind: precondition
  - Statement: every merge whose ring proof does not verify under `RingCircuit::Merge` over the statement of INV-MERGE-12 returns Err, and no SPP CPI happens in it.
  - Location: `programs/custom-ring/src/instructions/settlement/merge.rs:91-99` (`fn process_merge_ix`)
  - Error: `CustomRingError::ProofVerificationFailed = 8101` / `CustomRingError::InvalidRingProofPoint = 8238`
  - Severity: Critical
  - Suggested test: negative with a valid SPP merge proof and a wrong ring proof, harness: localnet

- [x] **INV-MERGE-14: a merge demands no co-signer**
  - Covered by: `program-tests/custom-ring/tests/rules/merge_rules.rs` `a_transfer_scoped_co_signer_demands_nothing_of_a_merge` (LiteSVM)
  - Kind: state
  - Statement: for every ring configuration, a merge reads no co-signer record and no approval.
  - Location: `programs/custom-ring/src/instructions/settlement/merge.rs:38-100` (`fn process_merge_ix`)
  - Severity: Medium
  - Suggested test: positive with records of every scope, harness: LiteSVM

### CPI and Success

- [x] **INV-MERGE-15: the forwarded data is SPP's `merge_ring` over the decompressed body**
  - Covered by: `sdk-libs/client/tests/merge.rs` `the_forward_parses_as_merge_ring`, `the_view_rebuilds_the_exact_spp_body_from_the_compressed_point` (library)
  - Kind: postcondition
  - Statement: the SPP instruction data is exactly `RING_MERGE_TRANSACT || output_ring_data_hash || head || decompressed proof.b || tail`, with `head` and `tail` the merge data's own bytes.
  - Location: `programs/custom-ring/src/instructions/settlement/merge.rs:98-99` (`fn process_merge_ix`), `program-libs/interface/src/instructions/merge.rs:55-62, 81-85` (`fn forwarded`, `fn decompressed`)
  - Severity: Critical (SPP's merge proof binds `output_ring_data_hash`, the ring statement does not)
  - Suggested test: golden vector, harness: library

- [ ] **INV-MERGE-16: a successful merge calls SPP once and writes no account itself**
  - Partial coverage: `program-tests/custom-ring/tests/ring/merge.rs` `cli_merges_fragmented_custom_ring_notes`, `cli_merges_twenty_four_custom_ring_utxos` and `program-tests/custom-ring/tests/ring/relay.rs` `a_relayed_user_merge_lands_one_note_of_the_merged_value` (localnet) assert the merged note. No test asserts a single inner instruction or unchanged ring-owned accounts.
  - Kind: postcondition
  - Statement: after every successful merge, the transaction holds exactly one ring program inner instruction, the SPP call of INV-MERGE-15 over the forwarded list with `ring_auth` signing, and the ring program wrote no account.
  - Location: `programs/custom-ring/src/instructions/settlement/merge.rs:38-100` (`fn process_merge_ix`)
  - Severity: High
  - Suggested test: positive with inner instruction inspection, harness: localnet

- [ ] **INV-MERGE-17: a merge changes neither the ring account nor the tracking registry**
  - Not covered.
  - Kind: frame
  - Statement: after every merge, the ring account and the tracking registry are unchanged in data and lamports.
  - Location: `programs/custom-ring/src/instructions/settlement/merge.rs:38-100` (`fn process_merge_ix`)
  - Severity: Medium
  - Suggested test: positive with an account snapshot, harness: localnet

- [ ] **INV-MERGE-18: the ring relies on SPP for the merge's value and ownership**
  - Not testable here. INSUFFICIENT_INFO: the owner, asset and sum equality of the inputs and the output, the nullifier insertion and the binding of `output_ring_data_hash` live in SPP's `merge_ring` processor and its merge circuit, outside the analyzed source.
  - Kind: state
  - Statement: the ring program performs no check over the merged notes' owner, asset or value, and settles only what SPP's merge proof accepts.
  - Location: `programs/custom-ring/src/instructions/settlement/merge.rs:1-15` (module contract)
  - Severity: Critical
  - Suggested test: none in this crate, harness: SPP's own suites
