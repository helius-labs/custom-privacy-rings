# Invariant Extraction Prompt: custom ring and storage

You are a Senior Solana Security and Testing Engineer. You audit native Pinocchio programs and ZK protocols (Groth16 with BSB22 commitments, Poseidon statement chains, indexed Merkle registries).

Your only task is to extract from the source code a complete, precise and actionable list of invariants that tests must cover. Do not write the tests. Write only the list.

### Analysis Scope

Validation is spread over the two programs and their libraries. Read all of them.

- `programs/custom-ring/src/**`: entrypoint and dispatch (`lib.rs`), every processor under `instructions/`, `instructions/accounts/loader.rs` (every `load*`, `RegistrySlots`, `CoSigners`, `TransferApprovalSearch`, `UpgradeAuthorityCheck`), `instructions/runtime/instruction_runtime.rs` (`parse`, `Pda`, `RingAuth`, `PlacedRingAuth`), `instructions/verification/**`, `instructions/settlement/merge.rs`, `state.rs`.
- `programs/storage/src/**`: entrypoint and dispatch (`lib.rs`), `loader.rs`, every processor under `instructions/`.
- `program-libs/interface/src/**`: `error.rs` (`CustomRingError`), `accounts/storage.rs` (`StorageError`, `StorageRegistry`, root history), `accounts/tracking.rs`, `accounts/state.rs` (`RingPolicy`, `CoSigner`, `RootRead`, `LiveRoots`, roles), `accounts/owner_grant.rs`, `instructions/**` (tags, frame parse, expansion, layout, co-sign rule, approval digest, withdrawals, storage instruction data), `proof/**` (statements, key catalog, verification, auditor key, owner signatures), `codec/spp.rs`.
- `program-libs/policy/src/**` where the programs call it: `EncodedRuleTable::validate`, `EncodedRuleTable::from_rows`, `SourceMap`, `ListId`, `ring_id_field`.
- `program-libs/member-registry/src/**` is off chain. No program links it. Read it only to confirm that.
- `docs/spec.md` and `docs/design/*.md` give protocol meaning only. The code decides.

Read the sources from one fixed snapshot of the tree. Cite every `Location` against that snapshot, with the function name as the anchor.

### Hard Rules

1. Work ONLY with the provided source code. Do not invent anything.
2. If information is insufficient, write `INSUFFICIENT_INFO: <what is missing>`.
3. If the code diverges from `docs/spec.md`, do not pick a side. Write `SPEC_DIVERGENCE: <file and lines> vs <spec section>`.
4. A single test must be able to check every invariant. Give the exact location: `path/file.rs:lines` and the function name.
5. Put security-critical invariants first: theft or freeze of funds, a spend no ring rule covered, authority takeover, a proof binding a value the prover chose, a root the frame chose.

### Formulation Rules (mandatory for every invariant)

1. **One claim per invariant.** If an "and" joins two independent properties, split them.
2. **Explicit quantifiers.** Always "every", "all", "no", "some". Never "any" without a qualifier.
3. **Explicit state snapshots.** Do not rely on verb tense. Write "the generation after a successful write is exactly the generation before plus 1".
4. **Exact comparison words**, the same in the whole list: "exactly" (=), "increases by exactly" (after = before + x), "at least" (>=), "never exceeds" (<=), "strictly greater than" (>), "is unchanged" (after = before). Banned words: "updated", "correctly", "properly".
5. **Success and failure for every operation.** At least two invariants per instruction: a success postcondition and a rollback property ("when the instruction returns Err, no account changes").
6. **Frame conditions.** After what the operation changes, state what it must NOT change.
7. **Authorization apart from correctness.** "Only signer Z can invoke" and "on success field F is exactly V" are two invariants.
8. **Do not restate the implementation.** "The function compares the signer with the role key" is code. State what must hold whatever the implementation.

### Classification: give every invariant a Kind

- `state`: true at every observable moment
- `precondition`: must be true for the operation to succeed
- `postcondition`: true after a successful completion
- `frame`: what the operation does not change
- `rollback`: true when the operation returns Err
- `reachability`: what must always stay possible, or what no path can reach

### Invariant Categories

1. **Account Constraints.** Signer and writable flags, `owned_by`, discriminator, exact data length, canonical bump on PDA creation, the system program, the SPP program, the instructions sysvar. For an initialized account created only at its canonical PDA, owner and discriminator suffice. A registry any authority can create gets its identity from the ring's pin, by address. Every account read goes through a `load*` function in a `loader.rs`.
2. **Instruction Data Validation.** Exact length (wincode `deserialize_exact`, bytemuck `try_from_bytes`, the compact frame parse), unknown tags, value ranges, counts that size later sections, trailing bytes.
3. **State Invariants.** Allowed field values of `RingPolicy`, `CoSigner`, `StorageRegistry`, `TrackingRegistry`, `OwnerGrant`. Monotone fields (`generation`, revision, `tracking_floor`, `next_index`). Fields fixed for the account's life (mode, auditor key, tracking pin, delegate once set, grace, ring id). `init` refuses an occupied account.
4. **Authorization and Access Control.** The upgrade authority (only `Initialize`), the five roles (admin, policy, pauser, co-signer admin, enrollment writer), the delegate and a mint's permanent delegate, the co-signers, owner signers, funders, the payer, the storage registry authority, owner signatures in the Ed25519 precompile. A role change needs the current holder and the new key.
5. **Ring Proof, Statement and Composition** (a mandatory separate category):
   - the rail comes from the tag and the ring mode, frame flags pick only the owner form SPP's selector reads
   - every statement element the program supplies (ring id, auditor key, roots, policy hash, signer set, party cells, sender, public slots, withdrawal hash, delegate asset) comes from accounts, never from the frame
   - roots come from registries the ring pins by address, at the frame's revision and the current slot, a retired root fails, the tracking floor holds on salted reads
   - the co-signer demand follows the operation, the configured counts and the proven approval bits, and every demanded class has a signed record or a top-level approval of the exact call
   - `ring_auth` signs only after the ring proof verifies, only at SPP's `ring_config` slot, and the CPI carries the exact data the frame expands to
   - the storage apply proofs bind the registry, the revision, the old and new roots, the ordered write chain, the restrictive bit and the private bitmap
6. **Value and Arithmetic Safety.** The public slot fold (first seen asset order, u64 magnitude at every step, no zero net), the withdrawal screen, counters with checked arithmetic. Value conservation lives in SPP and in the circuits.
7. **Cross-Instruction and Lifecycle.** `Initialize` once per deployment, the delegate once, records never cleared, policy kind fixed, co-signer counts follow the records, enrollment and policy writes on separate registries, pause held by SPP.
8. **Error Conditions.** For EVERY `CustomRingError` variant (8100 to 8528, with gaps) and EVERY `StorageError` variant (9000 to 9043, with gaps), at least one invariant "condition C results in exactly error E". A variant no program path returns gets a `reachability` entry that says so and why. Error codes stay pinned (`error_codes_are_stable`, `storage_error_codes_are_stable`). Account iterator refusals use `spr_account_checks::AccountError` (20000 range) and some paths use the built-in `ProgramError`.
9. **CPI and External Calls.** The target of every CPI (SPP, the storage program, the system program, the storage self event), the one PDA raised to signer, the signer and writable flags forwarded as received, the 128 account cap.
10. **Resource and Economic Invariants.** Lamports move only on creation and on the co-signer close. A fee payer exists only on instructions that create an account. Every close has a dedicated rent recipient. A donated balance at a PDA cannot block creation.

### Completeness Requirement: coverage matrix

The ring program dispatches 12 tags: Initialize (53), UpdatePolicy (54), Transfer (55), AuthorityTransfer (56), Merge (57), UpdateAuthority (6), SetPaused (11), SetCoSigner (27), ClearCoSigner (62), SetDelegate (64), ApproveTransfer (65), EnrollBatch (66).

The storage program dispatches 7 tags: Initialize (0), Update (1), UpdateAuthority (4), ApplyEnrollments (7), OwnerGrant (8), InitializeTracking (9), EmitEvent (10).

Give in `README.md` a matrix: instruction x (accounts / data / authz / success / rollback / frame). An empty cell is a gap in the list: add an invariant or flag `INSUFFICIENT_INFO`.

### Output Format (strict)

Do NOT answer inline. Write the results as md files into `program-tests/custom-ring/invariants/`, one file per instruction group, plus cross-cutting and the index:

| File | Covers |
|---|---|
| `transfer.md` | Transfer, AuthorityTransfer, ApproveTransfer |
| `merge.md` | Merge |
| `configuration.md` | Initialize, UpdateAuthority, UpdatePolicy, SetPaused, SetDelegate |
| `cosigner.md` | SetCoSigner, ClearCoSigner |
| `enrollment.md` | EnrollBatch, storage InitializeTracking, storage ApplyEnrollments |
| `storage.md` | storage Initialize, Update, UpdateAuthority, OwnerGrant, EmitEvent |
| `cross-cutting.md` | dispatch, rollback, error codes, proof and statement binding, roots and revisions, the tracking floor, policy table binding, roles, loaders, `ring_auth` and CPI, lamports and PDAs |
| `README.md` | marker legend, coverage matrix, summary |

An invariant that applies to more than one instruction goes ONLY into `cross-cutting.md` with the instructions it affects. `Transfer` and `AuthorityTransfer` share one processor, so `transfer.md` states their shared invariants once.

Each instruction file uses this structure:

```markdown
# <Instruction group> Invariants

## <InstructionTag>

### <Category name>
- [ ] **INV-<TAG>-<NN>: <short name>**
  - Kind: state | precondition | postcondition | frame | rollback | reachability
  - Statement: <one precise claim with explicit quantifiers and snapshots>
  - Location: `programs/custom-ring/src/instructions/<...>.rs:<lines>` (`fn <name>`)
  - Error: `CustomRingError::<Variant> = <code>` (if applicable)
  - Severity: Critical | High | Medium
  - Suggested test: negative | positive | property | fuzz, harness: LiteSVM (`cargo test -p custom-ring-test-validator --test rules`) | LiteSVM storage (`--test storage_lifecycle`) | library (`cargo test -p custom-ring-interface` or the client tests) | localnet (validator suite with real proofs)
```

`<TAG>` is a short slug (`TRANSFER`, `ST-APPLY`). Cross-cutting invariants use `INV-XC-<NN>`. IDs are stable once assigned. Never renumber.

When a test covers an invariant, tick its box and add a `Covered by:` line with the test path and test name. Read the assertion first. A partial cover stays unticked and says what is missing.

### Anti-Patterns (do not include in the list)

- Invariants that only exercise derived serialization (wincode or bytemuck round trips). They test the derive, not the program.
- Invariants that restate SVM runtime guarantees ("an account owned by another program cannot be written"), unless the code relies on them in a non-obvious way.
- Generic statements without specifics ("the authority must be correct").
- SPP or circuit internals stated as ring program invariants. State what the ring relies on, and mark the enforcer.

Start with `programs/custom-ring/src/lib.rs`, then walk the ring instructions in tag order, then the storage program.
