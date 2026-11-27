# Custom Ring and Storage Invariants

Test-coverage checklist for the two on-chain programs of this repository, the custom ring program (`programs/custom-ring`) and the storage program (`programs/storage`). Every entry comes from the program sources and the interface library they link. `docs/spec.md` gives protocol meaning, and the code decides. `PROMPT.md` holds the extraction rules.

A line number in a `Location` is a hint. The function name is the anchor, and an edit that moves the lines keeps it.

## Markers

- `- [x]` a test asserts the invariant with its exact refusal or value. `Covered by` names the test and its harness.
- `- [ ]` partial or uncovered. `Partial coverage` names what a test asserts and what it leaves out. `Not covered` names the gap. `Not testable here` marks an invariant the ring relies on and another component enforces, with its `INSUFFICIENT_INFO`.
- `- [-]` no program path reaches it. The entry pins that fact and its reason.

Harness labels in `Covered by`:

| Label | Meaning |
|---|---|
| LiteSVM | `cargo test -p custom-ring-test-validator --test rules` over the deployed SBF binaries, no prover, a ring proof stops at its pairing |
| LiteSVM storage | `--test storage_lifecycle` over the storage binary, with committed real apply proofs |
| library | the interface code the program runs, through `program-libs/interface/tests` or `sdk-libs/client/tests` |
| unit | an inline `#[cfg(test)]` module of the interface crate |
| localnet | the validator suite with real proofs (`tests/ring/*`, `ring_shapes`) |

A library cover tests the same function the program calls. It does not show the program reaches that function, so a matrix cell that lists only library-covered entries still deserves an on-chain test. A cover through `validate_under` tests the gate of host-built data, which the program does not call. `program-libs/interface/tests/transact_frames.rs` pins that `parse` accepts no mutated vector frame that gate refuses.

## Files

| File | Covers |
|---|---|
| `transfer.md` | Transfer (55), AuthorityTransfer (56), ApproveTransfer (65) |
| `merge.md` | Merge (57) |
| `configuration.md` | Initialize (53), UpdatePolicy (54), UpdateAuthority (6), SetPaused (11), SetDelegate (64) |
| `cosigner.md` | SetCoSigner (27), ClearCoSigner (62) |
| `enrollment.md` | EnrollBatch (66), storage InitializeTracking (9), storage ApplyEnrollments (7) |
| `storage.md` | storage Initialize (0), Update (1), UpdateAuthority (4), OwnerGrant (8), EmitEvent (10) |
| `cross-cutting.md` | dispatch, data parsing, rollback, error codes, roles, loaders, roots and revisions, the tracking floor, policy table binding, `ring_auth` and CPI, proof system, lamports and PDAs, the build, statement hashes |

The storage program's `EmitEvent` (tag 10) is a seventh storage tag, and `storage.md` holds it. `OwnerGrant` reads a tracking registry but grants a reader, so it stays in `storage.md`.

ID prefixes: `INV-TRANSFER`, `INV-AUTH-TRANSFER`, `INV-APPROVE`, `INV-MERGE`, `INV-INIT`, `INV-UPDATE-POLICY`, `INV-UPDATE-AUTH`, `INV-SET-PAUSED`, `INV-SET-DELEGATE`, `INV-SET-COSIGNER`, `INV-CLEAR-COSIGNER`, `INV-ENROLL`, `INV-ST-INIT-TRACK`, `INV-ST-APPLY`, `INV-ST-INIT`, `INV-ST-UPDATE`, `INV-ST-UPDATE-AUTH`, `INV-ST-GRANT`, `INV-ST-EVENT`, `INV-XC`. IDs are stable once assigned. Never renumber.

`Transfer` and `AuthorityTransfer` share `process_transfer_ix`, so every `INV-TRANSFER-*` entry holds for both tags unless it names a rail. The matrix lists the shared entries in both rows.

## Coverage Matrix

| Instruction | File | Accounts | Data | Authz | Success | Rollback | Frame |
|---|---|---|---|---|---|---|---|
| Initialize (53) | `configuration.md` | INV-INIT-03..07, INV-XC-43, INV-XC-44 | INV-INIT-08..12, INV-XC-06, INV-XC-27..30 | INV-INIT-01, INV-INIT-02, INV-XC-17, INV-XC-25 | INV-INIT-13..16 | INV-INIT-17, INV-XC-08, INV-XC-09 | INV-INIT-18 |
| UpdatePolicy (54) | `configuration.md` | INV-XC-18, INV-XC-19, INV-XC-30 | INV-UPDATE-POLICY-02, -04, -05, -08, INV-XC-06, INV-XC-27..29 | INV-UPDATE-POLICY-01, INV-XC-16 | INV-UPDATE-POLICY-03, -06, -07, -09..12, INV-XC-25 | INV-UPDATE-POLICY-13, INV-XC-08 | INV-UPDATE-POLICY-14, -15 |
| Transfer (55) | `transfer.md` | INV-TRANSFER-26, -36, -42, -43, -45, -47, -48, INV-XC-18..20, INV-XC-32, INV-XC-34, INV-XC-36, INV-XC-37 | INV-TRANSFER-02..18 | INV-TRANSFER-01, -25, -27..35, -37, -38 | INV-TRANSFER-19..23, -39..41, -44, -46, -49..55, INV-XC-22..24 | INV-TRANSFER-52, INV-XC-08, INV-XC-09 | INV-TRANSFER-56 |
| AuthorityTransfer (56) | `transfer.md` | INV-AUTH-TRANSFER-05, INV-TRANSFER-43, INV-XC-18..20, INV-XC-32, INV-XC-34, INV-XC-36, INV-XC-37 | INV-TRANSFER-03..18, INV-AUTH-TRANSFER-08 | INV-AUTH-TRANSFER-01, -02, -04, -06, INV-TRANSFER-24 | INV-AUTH-TRANSFER-03, -07, -09, INV-TRANSFER-19, -23, -50..55 | INV-TRANSFER-52, INV-XC-08 | INV-TRANSFER-56 |
| Merge (57) | `merge.md` | INV-MERGE-05..09, INV-XC-20, INV-XC-32, INV-XC-34, INV-XC-36 | INV-MERGE-01..04 | INV-MERGE-08, INV-MERGE-14 | INV-MERGE-10..13, -15, -16 | INV-MERGE-13, INV-XC-08 | INV-MERGE-17 |
| UpdateAuthority (6) | `configuration.md` | INV-XC-13, INV-XC-18 | INV-UPDATE-AUTH-03, INV-XC-06 | INV-UPDATE-AUTH-01, -02, INV-XC-16 | INV-UPDATE-AUTH-04..06, INV-XC-17 | INV-XC-08 | INV-UPDATE-AUTH-07 |
| SetPaused (11) | `configuration.md` | INV-SET-PAUSED-02, INV-XC-36 | INV-SET-PAUSED-03, INV-XC-06 | INV-SET-PAUSED-01, INV-XC-16 | INV-SET-PAUSED-04, -05 | INV-XC-08, INV-XC-09 | INV-SET-PAUSED-06 |
| SetCoSigner (27) | `cosigner.md` | INV-SET-COSIGNER-02, -03, -06, -09, INV-XC-43, INV-XC-44 | INV-SET-COSIGNER-04, -05, INV-XC-06 | INV-SET-COSIGNER-01, INV-XC-16 | INV-SET-COSIGNER-07, -08, -10..12 | INV-XC-08 | INV-SET-COSIGNER-13 |
| ClearCoSigner (62) | `cosigner.md` | INV-CLEAR-COSIGNER-02, -03 | INV-CLEAR-COSIGNER-04 | INV-CLEAR-COSIGNER-01, INV-XC-16 | INV-CLEAR-COSIGNER-05..07 | INV-XC-08 | INV-CLEAR-COSIGNER-08 |
| SetDelegate (64) | `configuration.md` | INV-XC-13, INV-XC-18 | INV-SET-DELEGATE-02 | INV-SET-DELEGATE-01..04, INV-XC-16, INV-XC-17 | INV-SET-DELEGATE-05, -06 | INV-XC-08 | INV-SET-DELEGATE-07 |
| ApproveTransfer (65) | `transfer.md` | INV-APPROVE-03 | INV-APPROVE-04 | INV-APPROVE-01, -02 | INV-APPROVE-05 | INV-XC-08 | INV-APPROVE-06 |
| EnrollBatch (66) | `enrollment.md` | INV-ENROLL-02..04 | INV-ST-APPLY-01 (storage parses the forwarded data) | INV-ENROLL-01, -06, INV-XC-16 | INV-ENROLL-05, -08 | INV-XC-08, INV-XC-09 | INV-ENROLL-07 |
| storage Initialize (0) | `storage.md` | INV-ST-INIT-01, -03, -05, INV-XC-43, INV-XC-44 | INV-ST-INIT-04, INV-XC-07 | INV-ST-INIT-02 | INV-ST-INIT-06..09 | INV-XC-08 | INV-ST-INIT-10 |
| storage Update (1) | `storage.md` | INV-ST-UPDATE-06, -08, -12, -13 | INV-ST-UPDATE-01..05, INV-XC-48 | INV-ST-UPDATE-07, -09 | INV-ST-UPDATE-10, -11, -14..20 | INV-ST-UPDATE-21 | INV-ST-UPDATE-22 |
| storage UpdateAuthority (4) | `storage.md` | INV-ST-UPDATE-AUTH-05 | INV-ST-UPDATE-AUTH-01, INV-XC-07 | INV-ST-UPDATE-AUTH-02, -03 | INV-ST-UPDATE-AUTH-04 | INV-XC-08 | INV-ST-UPDATE-AUTH-06 |
| storage ApplyEnrollments (7) | `enrollment.md` | INV-ST-APPLY-02..04 | INV-ST-APPLY-01, -09..11, -13, INV-XC-48 | INV-ST-APPLY-05..08 | INV-ST-APPLY-12, -14..17, -19, -20 | INV-ST-APPLY-18, INV-XC-09 | INV-ST-APPLY-21 |
| storage OwnerGrant (8) | `storage.md` | INV-ST-GRANT-01, -03, -04, -08, -10, INV-XC-43, INV-XC-44 | INV-ST-GRANT-05..07, INV-XC-07 | INV-ST-GRANT-02 | INV-ST-GRANT-09, -11 | INV-XC-08 | INV-ST-GRANT-12 |
| storage InitializeTracking (9) | `enrollment.md` | INV-ST-INIT-TRACK-01, -03, -07, INV-XC-43, INV-XC-44 | INV-ST-INIT-TRACK-04..06, INV-XC-07 | INV-ST-INIT-TRACK-02 | INV-ST-INIT-TRACK-08..10 | INV-XC-08 | INV-ST-INIT-TRACK-11 |
| storage EmitEvent (10) | `storage.md` | reads no account (INV-ST-EVENT-01) | takes every data (INV-ST-EVENT-01) | permissionless by design, INV-ST-EVENT-02 bounds the trust | INV-ST-EVENT-01 | never returns Err | INV-ST-EVENT-01 |

Rows that apply to every instruction of a program and stay out of the cells: INV-XC-01, INV-XC-02, INV-XC-04 (dispatch), INV-XC-05 (one ring per deployment), INV-XC-10, INV-XC-11 (error codes), INV-XC-12, INV-XC-13 (error conversions), INV-XC-14, INV-XC-15 (unreachable legs), INV-XC-21 (one load per account), INV-XC-31, INV-XC-33, INV-XC-35 (`ring_auth` and CPI targets), INV-XC-38, INV-MERGE-18 (what SPP enforces), INV-ST-APPLY-22, INV-ST-UPDATE-23 (what the apply circuits enforce), INV-XC-39..42 (proof system), INV-XC-45, INV-XC-46 (lamports, payers) and INV-XC-47 (the build). INV-XC-26 covers the two storage root moves.

## Summary

- Total invariants: 305
  - `transfer.md`: 71 (Transfer 56, AuthorityTransfer 9, ApproveTransfer 6)
  - `merge.md`: 18
  - `configuration.md`: 53 (Initialize 18, UpdatePolicy 15, UpdateAuthority 7, SetPaused 6, SetDelegate 7)
  - `cosigner.md`: 21 (SetCoSigner 13, ClearCoSigner 8)
  - `enrollment.md`: 41 (EnrollBatch 8, InitializeTracking 11, ApplyEnrollments 22)
  - `storage.md`: 53 (Initialize 10, Update 23, UpdateAuthority 6, OwnerGrant 12, EmitEvent 2)
  - `cross-cutting.md`: 48
- Severity: Critical 101, High 93, Medium 111.
- Coverage: covered 191, partial 43, uncovered 64, not testable here 5, unreachable 2.

| File | Covered | Partial | Uncovered | Not testable | Unreachable |
|---|---|---|---|---|---|
| `transfer.md` | 61 | 6 | 4 | 0 | 0 |
| `merge.md` | 11 | 3 | 3 | 1 | 0 |
| `configuration.md` | 37 | 5 | 11 | 0 | 0 |
| `cosigner.md` | 10 | 3 | 8 | 0 | 0 |
| `enrollment.md` | 25 | 3 | 12 | 1 | 0 |
| `storage.md` | 26 | 6 | 19 | 2 | 0 |
| `cross-cutting.md` | 21 | 17 | 7 | 1 | 2 |

| Severity | Covered | Partial | Uncovered | Not testable | Unreachable |
|---|---|---|---|---|---|
| Critical | 81 | 13 | 3 | 4 | 0 |
| High | 68 | 15 | 9 | 1 | 0 |
| Medium | 42 | 15 | 52 | 0 | 2 |

Every `CustomRingError` variant (82, codes 8100 to 8528) and every `StorageError` variant (29, codes 9000 to 9043) has at least one invariant that names the condition returning it, or an `INV-XC-14` or `INV-XC-15` item that names why no path returns it.

### SPEC_DIVERGENCE

1. `programs/storage/src/instructions/owner_grant.rs:21-47` accepts every signing owner, a wallet key included, vs `docs/spec.md` Accounts item 6 and Instructions, Storage Program item 5. The spec scopes `OwnerGrant` to a program owner's activity. See INV-ST-GRANT-11.

### INSUFFICIENT_INFO

1. SPP's checks behind every ring settlement (membership, nullifiers, conservation, expiry, activation, pause, the authority rail flag, settlement accounts) live under `zolana/`. INV-XC-38, INV-MERGE-18.
2. The tree semantics of both apply circuits live in `prover/circuits/registry`. INV-ST-APPLY-22, INV-ST-UPDATE-23.
3. The trust rule for a `StorageEvent` lives in the ring indexer. INV-ST-EVENT-02.
