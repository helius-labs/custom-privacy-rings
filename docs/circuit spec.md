# Custom ring specification and implementation status

This is the PR #17 circuit specification, checked against the code. Each section ends with its status. [requirements.md](requirements.md) does the same for the target requirements.

## How to read the status

- **Implemented:** the code at the merged ring-core head `823e355f` has the mechanism.
- **Partially:** part is merged and the rest is in progress, or the code covers only part of the item. The block names both.
- **Deferred:** not built and not in progress.
- Code paths are relative to the repository root at `823e355f`. A path marked "in progress" is uncommitted core work or an unmerged SDK lane.
- Nothing here is validated with real proofs on release keys. At `823e355f` only the record update, `RegisterKey` and deposit circuits have a wired verifying key. Transfer and merge proofs are refused with `UnsupportedCircuit` until their keys exist. No section claims settlement or release readiness.

## Custom rings end to end

A custom ring is a set of SPP notes that only one ring program can move. Each ring note commits the ring id. SPP accepts a ring rail only when the ring PDA signs the CPI, and only the ring program can sign for that PDA. Every check the ring program makes therefore applies to every move of a ring note.

Every transfer runs on a rail and carries two proofs. The SPP proof shows the spend is valid. The ring proof shows the audit, the policy and the approvals. One transfer circuit serves every rail, and statement bits that the program sets pick the rail:

1. Signer rail: each owner signs the Solana transaction.
2. In-proof rail: the owner signs inside the SPP proof, so Ring RPC can pay and send.
3. Hidden rail: on an anonymous ring, a hidden owner signs inside the SPP proof.
4. Public-owner rail: on an anonymous ring, every owner signs in the open and recipients stay hidden.
5. Delegate rail: the permanent delegate moves notes with no owner signature.

A deposit, a merge and a record update each have their own circuit.

Lists and asset settings live in policy record trees that the Storage program keeps. A leaf is a member record with list bits and, for an asset, its mint settings. The tree is sorted, so a proof can show that a member is on a list or is not. The ring account binds its own records tree and at most one curator tree. The program reads their roots live on every transfer.

The policy is a rule table of at most 16 rules, folded into one Poseidon hash on the ring account. A rule screens the sender, every output owner or every asset by presence on grant lists or absence from a deny list. A guard can limit a rule to amounts above a threshold. Mint settings add a per-transfer outflow cap and approval thresholds. The transfer proof checks all of it on private inputs.

The auditor holds one key for the ring's life. Every transfer proves an AES stream to that key with one record per published output: kind, amount, owner key, mint and nullifier key. Output blindings derive from a secret in the same stream, so the auditor rebuilds every output commitment. The auditor links spends by deriving nullifiers from each owner's nullifier secret. Recipient encryption stays outside the proof. Ring RPC serves the auditor's view to authorized readers.

An owner enrolls before it receives or spends ring notes under a nonzero nullifier key. It seals its nullifier secret and list salt to the auditor, and a `RegisterKey` proof shows the seal. Merged code records enrollments in Storage accounts for a per-ring tracking tree. The agreed target moves each enrollment to an immutable SPP compressed credential, and that switch is in progress. A note under the zero nullifier key needs no enrollment, so program-owned notes use it.

A ring can name a permanent delegate once. The delegate moves any ring note without its owner and outside every rule. The audit stream still applies.

A co-signer approves classes of operations. The transfer proof sets approval bits when an owner's outflow or a public withdrawal passes the asset's threshold. The program then demands a co-signer for each set bit. Ring RPC co-signs only within per-asset window limits.

As relay, Ring RPC proves, co-signs, pays for and sends wallet operations. The wallet seals the audit stream and signs. It reads no Merkle path. Both SDKs keep a direct path without Ring RPC.

## Circuits

The catalog has **6 circuit families and 28 keys**, one Groth16 key per shape.

| Circuit family | Keys | Shapes |
|---|---:|---|
| Transfer | 12 | One circuit for every rail. 4 narrow, 4 wide and 4 entry shapes |
| Record updates (`ApplyRecords`) | 5 | 1, 2, 4, 8 or 16 writes |
| Enrollment apply (`ApplyEnrollments`) | 5 | 1, 2, 4, 8 or 16 enrollments |
| `RegisterKey` | 1 | One sealed nullifier secret and list salt |
| Deposit | 4 | 1, 2, 4 or 8 outputs |
| Merge | 1 | Every SPP merge width |
| **Total** | **28** | |

- **Status:** Partially.
- **Why:** The catalog and circuits are merged, but only 10 keys have a wired verifying key and the key lockfile still names the PR #17 key set.
- **How:** One Groth16 key per shape. Rail bits in the statement replace the PR #17 per-rail families, so confidential, anonymous and delegate transfers share one key per width.
- **Argument:** A frame that matches no catalog row fails with `RingShapeUnkeyed` or `UnsupportedCircuit`, so the program never verifies under a wrong key.
- **Code:** `program-libs/interface/src/proof/key_catalog.rs`, `program-libs/interface/src/proof/ring_keys.rs`, `program-libs/interface/src/verifying_keys/mod.rs`, `prover/circuits/transfer/shapes.go`.

Pending: the credential switch deletes the `ApplyEnrollments` family. The Go catalog in progress already drops it, and the Rust catalog in progress still keeps it.

### Transfer key shapes

Each key fixes its output count, input capacity, withdrawal slots and owner rows. A wide key fits a public and a salted record for each owner a transaction can name. A narrow key fits a public record for each. An entry key also opens default-ring inputs.

| Kind | Outputs | Inputs | Withdrawal slots | Owner rows |
|---|---:|---:|---:|---:|
| Narrow | 2 | 24 | 4 | 4 |
| Narrow | 4 | 24 | 4 | 4 |
| Narrow | 8 | 16 | 4 | 4 |
| Narrow | 16 | 5 | 8 | 5 |
| Wide | 8 | 24 | 32 | 24 |
| Wide | 16 | 5 | 8 | 5 |
| Wide | 16 | 8 | 8 | 8 |
| Wide | 2 | 49 | 4 | 12 |
| Entry | 4 | 8 | 4 | 8 |
| Entry | 8 | 2 | 4 | 2 |
| Entry | 8 | 4 | 4 | 4 |
| Entry | 2 | 24 | 32 | 12 |

- **Status:** Implemented.
- **Why:** The Rust catalog and the Go shapes list the same 12 rows.
- **How:** A frame takes the first row that holds its SPP width. Off entry, the rows reach every SPP width: 2 outputs up to 49 inputs, 4 and 8 outputs up to 24, 16 outputs up to 8. Entry is narrower and has no 16-output key. The SDKs refuse a wider shape by name, and the sender merges first.
- **Argument:** One key per output width serves all rails, and a test pins that (`every_rail_shares_the_key_of_its_output_width`).
- **Code:** `program-libs/interface/src/proof/key_catalog.rs`, `prover/circuits/transfer/shapes.go`.

### One ring proof per transfer

The transfer statement is one Poseidon `HashChain4` over the audit head, the rail bits, `approval_mask`, the output, record and data-tail chains, `private_tx_hash`, the party identities, the signer run, the policy hash, the own, curator and tracking roots, and the public legs. The element count closes the chain.

- **Status:** Partially.
- **Why:** The statement is merged. The input nullifier chain and the interleaved input layout are in progress.
- **How:** The program rebuilds every element from the frame it forwards to SPP and from live accounts, and the ring proof has one public input. The circuit reopens SPP's `private_tx_hash` over the same inputs and outputs, which ties the ring proof to the SPP proof.
- **Argument:** `HashChain4` alone does not fix the length. Appending the element count means a padded statement can never equal a shorter one.
- **Code:** `program-libs/interface/src/proof/statement.rs`, `prover/circuits/transfer/transfer.go`, `programs/custom-ring/src/instructions/transfer/verify.rs`.

In progress: the statement gains `input_nullifier_chain = HashChain4([n, nullifier_0, ..., nullifier_{C-1}])` over the nullifiers forwarded to SPP. The address chain is then derived from those nullifiers and an address bitmap, not supplied as a witness. Merged code still requires spent notes to come before address inputs. SPP lets them interleave, and the in-progress input map accepts that.

### Compact padding and dummies

- **Status:** Implemented.
- **Why:** The program and circuit accept `1 <= m <= N` published outputs with the rest omitted as a suffix.
- **How:** The omitted part is a suffix, so no slot index moves. A published dummy's hash enters the output chain, its blinding is held to the sealed secret, and its auditor record is kind 0 and all zero. The SDK gives every dummy a random tag and a random 72 B body on both ring modes.
- **Argument:** The circuit derives each record's kind from the slot's committed domain and ring id, so a prover cannot pass a real note off as a dummy.
- **Code:** `program-libs/interface/src/instructions/transact.rs`, `prover/circuits/audit/layout.go`, `sdk-libs/transaction/src/lib.rs`.

## Delivered audit

PR #17 described a confidential audit section and a separate anonymous one. One auditor stream now serves every rail.

1. **Auditor stream:** the transfer proof encrypts one AES-256-CTR stream to the auditor key. The key schedule is X25519 between the transaction viewing secret and the auditor key, then Poseidon with the label `CRING/adt` and a per-frame salt.
2. **Envelope:** the stream starts with the transaction viewing secret and `R`, the count of spent notes among the published inputs. An entry key adds the default-input bitmap and one opening per default-ring input.
3. **Records:** one record per published slot. Under `has_data`, a 64 B data tail per slot follows.
4. **Blindings:** every published output blinding derives from the sealed secret, the salt, the first nullifier and the slot index.

```rust
// Wire layout of one auditor record, 105 B, stream stride 7 AES blocks.
struct AuditRecord {
    kind: u8,              // 0 dummy, 1 ring output, 2 exit
    amount: u64,
    owner_key: [u8; 32],
    mint: [u8; 32],
    nullifier_pk: [u8; 32],
}
```

- **Status:** Implemented.
- **Why:** The stream, the records and the blinding derivation are proven in the transfer circuit.
- **How:** Verifiable encryption to the auditor, keyed by SPP's own transaction viewing secret plus a salt. Blindings are derived, not sent, so the stream carries no blinding.
- **Argument:** The auditor rebuilds each published commitment from its record and the derived blinding. A record moved to another slot or changed matches no commitment.
- **Code:** `prover/circuits/audit/audit.go`, `prover/circuits/audit/record.go`, `prover/circuits/shared/envelope.go`, `program-libs/interface/src/proof/audit_stream.rs`.

Superseded from PR #17: the stream no longer carries input hashes. Recipient bodies are not proven and are not shared with the auditor. No key escrow variant of the layout exists.

### Spent inputs and linkability

- **Status:** Partially.
- **Why:** Linking spends by derived nullifiers is merged. The address bitmap for interleaved inputs is in progress.
- **How:** A wallet has one nullifier secret, sealed to the auditor at enrollment. The auditor derives the nullifier of every note it opened and matches it against SPP's published nullifiers. Ring RPC keeps each published nullifier in one state: padding, external, resolved or unresolved.
- **Argument:** An SPP nullifier is a function of the note opening and the secret. With both, the auditor links every spend. A ring note spent in the frame needs no stream bytes, and a default-ring input on an entry key carries its opening.
- **Code:** `prover/circuits/audit/record.go`, `services/ring-rpc/src/operations/utxos.rs`, `services/ring-rpc/src/operations/spend_machine.rs`.

In progress: the envelope gains an 8-byte address bitmap after `R`, so notes and address inputs can interleave as SPP allows.

### Exits and withdrawals

- **Status:** Implemented.
- **Why:** An exit has a kind 2 auditor record, and a withdrawal adds no stream bytes.
- **How:** The exit record opens the created default-ring note for the auditor. The exit's recipient body is SPP's confidential output form (scheme 3), outside the ring proof. SPP publishes a withdrawal's asset, amount and destination.
- **Argument:** The auditor reads an exit from a proven record and needs nothing from the unproven body.
- **Code:** `prover/circuits/audit/record.go`, `program-libs/interface/src/codec/output.rs`.

## Anonymous audit

Recipient delivery on an anonymous ring uses these parts. The auditor message is the stream above.

```text
Anonymous transaction
+ Public context: transaction viewing key, salt, output count m, which slots carry no body
+ Output slots
|  + Output commitment
|  + Discovery tag, 8 B, an unproven hint
|  + Recipient body (scheme 10): blinding, amount, mint, 72 B, unproven
|  + Optional reply: claimed sender viewing key, owner key, nullifier key, data, unproven
+ Sender bundle (scheme 9), tag 32 B, unproven
|  + First change mint and amount, second change amount, output seed
|  + One recipient viewing key per published slot
+ Auditor stream, proven (see Delivered audit)
```

- **Status:** Implemented.
- **Why:** The codecs, schemes and scanners are merged. No proof constrains a body, tag, bundle or reply.
- **How:** Recipient encryption uses a slot-secret AES schedule outside the circuit. SPP's external data hash binds the bytes. A change slot has no body and the sender bundle rebuilds it. A dummy gets a random tag and a random body. Wallets find notes with one every-site scan that reads no tag.
- **Argument:** A bad tag hides nothing, because a full scan tries every body under every held key. A bad body cannot hide funds from the auditor, because the proven record still opens the note and Ring RPC serves it.
- **Code:** `program-libs/interface/src/codec/stream.rs`, `program-libs/interface/src/codec/output.rs`, `sdk-libs/client/src/operations/anonymous.rs`, `sdk-libs/ts/src/state/anonymous-sync.ts`.

Superseded from PR #17: schemes 11 and 12 are now 10 and 9. Recipient bodies, the bundle and tags are no longer proven. The scheme 1 and 2 readers are gone. Viewing keys are no longer proven against an enrolled address.

### Anonymous encryption sizes

PR #17 gave totals per frame. These fixed layouts are what the code pins.

| Part | Size |
|---|---|
| Envelope head | 33 B, 41 B on an entry key |
| Entry opening | 200 B per default-ring input |
| Auditor record | 105 B on the wire, 112 B stride |
| Data tail | 64 B per slot under `has_data` |
| Recipient body | 72 B |
| Sender bundle | 80 B plus 32 B per published slot |
| Reply | at least 97 B, one length per frame |
| Ring slot tag | 8 B |
| Exit tag and bundle tag | 32 B |

- **Status:** Implemented.
- **Why:** These lengths are constants in the interface crate.
- **How:** Fixed lengths are not transmitted, and record counters follow from the key, not from `m`.
- **Argument:** A fixed layout per key lets the program size and check a frame before any proof runs.
- **Code:** `program-libs/interface/src/proof/audit_stream.rs`, `program-libs/interface/src/codec/stream.rs`.

In progress: the envelope head grows by the 8-byte address bitmap.

### Notes

1. Optional reply metadata stays an unauthenticated claim, off by default. **Status:** Implemented.
2. Identify spends from derived nullifiers, not input hashes. **Status:** Implemented. Default-ring inputs still carry their openings on entry keys.
3. Mixed owners. **Status:** Implemented in the circuit, Partially in the SDKs. Owner rows run from 4 on narrow keys to 24 on the widest key, and entry keys hold 2 to 12 owners. The TypeScript SDK still refuses co-owners on an entry spend, and the Rust SDK does not.
4. The Poseidon KDF prototype measurements are historical and left out here. **Status:** Deferred. Recipient encryption left the circuit, so the prototype no longer applies.
5. "New recipient bodies, sender bundles and discovery tags are checked by the proof." **Status:** Deferred by decision. No proof constrains a body, bundle or tag.
6. Shared encryption schedule. **Status:** Implemented. Every ring stream, the merge record included, uses one Poseidon schedule under a 9-byte label. Exit bodies (scheme 3) are the one exception.
7. Compact padding and hidden dummies. **Status:** Implemented. Confidential dummies now also get a random tag and body, and data-bearing outputs are supported.
8. Ring RPC history coverage. **Status:** Implemented. Ring RPC walks finalized signatures, binds each indexed event to its canonical transaction and reports `historyThrough`. A spend is named by its published nullifier, not by an input hash.
9. `EscrowOwed` waiver. **Status:** Implemented as the zero-key rule. `EscrowOwed` is gone. A zero-key note `Poseidon(0)` needs no enrollment, and no address enrollment exists.
10. Separate confidential and anonymous families. **Status:** Implemented. One transfer circuit serves every rail.

## Merge

The ring merge circuit opens no input. It seals one opening record of the output and its seed to the auditor, under the shared schedule with the label `CRING/mrg`. Under a nonzero key it checks the owner's enrollment, and under the zero key the owner is published and signs in SPP.

- **Status:** Partially.
- **Why:** The circuit and the program are merged, but the merge key has no verifying key. The SPP merge selector it calls is refused in the checked-out zolana and absent from the pinned zolana commit.
- **How:** SPP's merge proof already binds every input to one owner, asset and sum. The ring proof only proves the output's record, so one ring key serves every SPP merge width.
- **Argument:** The ciphertext no longer grows with input count, and one record is enough to rebuild the output commitment.
- **Code:** `prover/circuits/merge.go`, `programs/custom-ring/src/instructions/settlement/merge.rs`, `zolana/programs/shielded-pool/src/instructions/merge/verify.rs`.

**Notes:**

1. Remove the per-input merge disclosure and encrypt only the output. **Status:** Implemented.
2. Merge runs no policy rule. **Status:** Implemented. It demands no co-signer.
3. Merge as an exception to the shared schedule. **Status:** Implemented. The merge record now uses the shared schedule.

## Policy

The rules are a `RuleTable`, and a `SourceMap` picks the registry for each list.

```rust
struct RuleTable { rules: [Rule; 16], len: u8 }
struct Rule { subject: Subject, source: RuleSource, guard: Guard }
enum Subject { OutputOwner = 1, Sender = 2, Asset = 4 }
enum RuleSource { Lists { present: ListSet, absent: ListSet } }
enum Guard { Always, AboveAmount(u64), AboveAmountByAsset }
struct SourceMap { slots: [PolicySource; 8] }
struct PolicySource { list_id: u8, registry: [u8; 32] } // zero registry selects the ring's own
```

`policy_hash` folds 44 field elements with Poseidon: the domain `CRTB`, the eight source slots, the salted-list bitmap, the rule count and sixteen rule rows. Unused rows are zero.

- **Status:** Implemented.
- **Why:** The table, its hash and its write-time validation are merged and mirrored in the circuit.
- **How:** The ring account stores the source slots, the encoded table and `policy_hash`. The circuit takes the table as private input and holds it to the hash. The program validates the rows when the table is written and refuses several curators, mixed polarity, absence over several lists, inverted polarity and a guard on a sender rule.
- **Argument:** The statement binds the hash and the program checked the table at write time, so a prover cannot substitute a rule.
- **Code:** `program-libs/policy/src/rule_table/mod.rs`, `prover/circuits/policy/table.go`, `program-libs/interface/src/accounts/state.rs`.

Superseded from PR #17: no encoding version field exists. `ExitDestination` is not a subject. An exit is screened as an output owner.

### What each rule screens

- **Status:** Implemented.
- **Why:** The policy sections of the transfer and deposit circuits apply the rules below.
- **How:**
  1. A sender rule checks every live owner row. Every live input owner needs a row, default-ring entry inputs included.
  2. An owner rule checks every live payment with a nonzero owner: ring outputs, exits and withdrawal legs.
  3. An asset rule checks every live payment asset. Without a guard it also checks every live input asset.
  4. A deposit runs owner and asset rules on its recipients and has no sender rows.
  5. A merge and a delegate move read no policy.
- **Argument:** The screening gate is `IsNonZero(policy_hash) * (1 - delegate)`. Both come from the statement, so no prover bit turns screening off.
- **Code:** `prover/circuits/policy/subjects.go`, `prover/circuits/policy/rules.go`, `prover/circuits/policy/deposit.go`, `prover/circuits/transfer/owners.go`.

Superseded from PR #17: a standalone deposit runs owner rules too. Input assets are screened. Delegate moves skip all rules and guards. Sender capacity is set per key, from 2 to 24 rows.

### Outflow caps and approval thresholds

An owner's `debit` of an asset is what its spent notes held, less what same-ring outputs return to it, never below zero. `charge` shares out what leaves beyond the summed debits among the owners whose funds came back. `outflow = debit + charge`. An exit and a public withdrawal count as leaving, an exit to the same owner included.

- **Status:** Implemented.
- **Why:** The outflow section is merged and gated by a nonzero own registry off the delegate rail.
- **How:** The cap and the co-sign bound come from the asset's own record. The peak outflow per asset must stay at or below a nonzero cap, and a passing co-sign bound sets the `OUTFLOW` bit in `approval_mask`.
- **Argument:** The charge term stops a deposit-funded return to the sender from hiding funds that leave. A sender who spends 1000, deposits 1000, gets 1000 back and exits 1000 is charged 1000.
- **Code:** `prover/circuits/policy/outflow.go`, `program-libs/policy/src/quantities.rs`.

### Withdrawal threshold

- **Status:** Implemented.
- **Why:** The PR #17 withdrawal exemption flag is retired, and the threshold now sets an approval bit.
- **How:** The circuit compares the gross public withdrawal of each asset, with no deposit subtracted, against the own record's `withdrawal_threshold`. Above it, the proof sets `APPROVE_WITHDRAWAL`.
- **Argument:** The circuit asserts `approval_mask` equal to the computed bits, not at least them, so a prover cannot drop a bit to avoid the approver.
- **Code:** `prover/circuits/policy/outflow.go`, `program-libs/policy/src/mint.rs`, `program-libs/interface/src/instructions/cosign.rs`.

### Lists

| List | Id | Read by | Written by |
|---|---:|---|---|
| Allow | 1 | presence | policy authority |
| Block | 2 | absence | policy authority |
| Frozen | 3 | absence | policy authority |
| RingViewing | 4 | not policy-readable | wallet |
| PrivateBlock | 5 | absence, salted key | policy authority |
| Reader | 6 | not policy-readable | policy authority or wallet grant |
| Approval | 7 | presence | policy authority |
| PrivateAllow | 8 | presence, salted key | policy authority |

A private entry is an ordinary record under the member key `Poseidon(CRPL, owner, salt)`. The owner's list tag `Poseidon(CRPT, salt)` is fixed at enrollment, so the salt is fixed too.

- **Status:** Partially.
- **Why:** Public and private lists are merged. The rule that forces an enrolled owner to open its salted key relies on a tracking floor that a write made before a pending enrollment applies can leave too low. The credential switch closes it.
- **How:** A salted key is pseudorandom to anyone without the owner's salt. An enrolled owner must open its salted key in a proof, and an owner proven not enrolled reads as absent from every private list. In progress, a private write to an owner key must prove the target's credential address in SPP's address tree and raises the source's `absence_floor` in the same instruction.
- **Argument:** A free salt would let a blocked owner choose an empty key. The tag fixed at enrollment removes that choice.
- **Code:** `program-libs/policy/src/list.rs`, `program-libs/policy/src/member.rs`, `prover/circuits/policy/subjects.go`.

### Non Policy Lists

1. **RingViewing:** a wallet-written flag. **Status:** Implemented as a flag. It has no viewing behavior.
2. **Recovery:** **Status:** Deferred. The list id is removed, and no recovery list exists.
3. **Reader:** grants decrypted reads through Ring RPC, ring-wide or for one wallet. **Status:** Implemented. A wallet grant uses the member key `Poseidon(CRWG, wallet, reader)`.
4. **Escrow:** **Status:** Implemented in the enrollment. The list id is removed, and the sealed secret lives in the enrollment.

**Notes:**

1. Registry zero selects the ring's own tree. **Status:** Deferred. It is still a sentinel.
2. RingViewing semantics. **Status:** Deferred. The flag carries no permission.
3. Recovery list. **Status:** Deferred. The list id is removed, not reserved.
4. Reader excluded from policy rules. **Status:** Implemented.
5. Tree permissions per purpose. **Status:** Partially. Each list reads exactly one source, so absence always reads one configured tree, and mint settings come from the own tree only. Inclusion from several approved trees is Deferred.

## Member policy

```rust
struct MemberRecord {
    member: [u8; 32],      // owner, asset, reader or wallet-reader grant
    next: [u8; 32],        // next member in sorted order
    lists: u64,            // list id i uses bit i - 1
    mint_policy: MintPolicy,
}
struct MintPolicy { threshold: u64, cap: u64, cosign_above: u64, withdrawal_threshold: u64, limits: AssetLimits }
struct AssetLimits {      // read by Ring RPC only
    sender_cap: u64, sender_window_slots: u64,
    volume_cap: u64, volume_window_slots: u64,
    deposit_cap: u64, deposit_window_slots: u64,
    withdrawal_cap: u64, withdrawal_window_slots: u64,
}
```

The record hashes 8 field elements with Poseidon. A present subject opens its record. An absent subject opens the predecessor with `member < subject < next` and reads zero list bits and settings.

- **Status:** Implemented.
- **Why:** The record, the indexed tree and the pools are merged.
- **How:** A sorted indexed Merkle tree gives membership and non-membership with one opening. Records open once under shared upper paths, and subject rows point into them, so a repeated subject reuses one record. Own-pool lookups read lists and mint settings. Curator lookups read list bits only.
- **Argument:** A row index selects a record but never its content, so reuse cannot pair a member with another member's record.
- **Code:** `program-libs/member-registry/src/record.rs`, `prover/circuits/policy/record_pool.go`, `prover/circuits/registry/registry.go`.

Pool capacity per key is `S + 2*O + W + salted + 3` own records and `S + 2*O + W + salted + min(W, 3)` curator records, for `S` owner rows, `O` outputs and `W` withdrawal slots.

Superseded from PR #17: `escrow_key`, `address_hash`, `AuthorizedAddress`, `EscrowKey` and `NullifierKeyPlaintext` are gone from the record. The record hashes 8 elements, not 10. Enrollment data lives in the enrollment (see below).

**Notes:**

1. Escrow beside `mint_policy`. **Status:** Implemented in the enrollment. Enrollment left the policy record. It is per ring, because `RegisterKey` binds the ring id. Merged enrollments still hold a per-ring viewing key. Both SDKs resolve viewing keys from zolana's `UserRecord`, and moving the ring to `UserRecord` comes with the credential switch, in progress.
2. Unified record pool. **Status:** Deferred. The decisions record keeps separate pools until a measured benefit exists.

## Enrollment and credentials

Enrollment seals an owner's nullifier secret and list salt to the auditor. PR #17 called this escrow. Only the auditor key opens the sealed secret.

```rust
// Target credential, one per ring and owner, an immutable SPP UniqueDataUtxo.
struct AuditCredential {
    ring_id: [u8; 32],
    member: [u8; 32],
    nullifier_pk: [u8; 32],
    list_tag: [u8; 32],              // Poseidon(CRPT, list_salt)
    auditor_pubkey: [u8; 33],
    eph_u: [u8; 32],
    ciphertext: [u8; 64],            // nullifier secret and list salt, sealed to the auditor
}
```

- **Status:** Partially.
- **Why:** Merged code records a verified enrollment in a Storage account, and applying it to the per-ring tracking tree needs an `ApplyEnrollments` key that is not wired. The SPP credential that replaces both is in progress and not integrated.
- **How:** `RegisterKey` proves verifiable encryption of the secret and salt to the auditor key, and binds the ring, the owner, the nullifier key and the list tag. Merged code stores the result in a Storage account for a later apply to a tracking tree. The target stores it as an SPP compressed credential owned by a ring PDA that only `Enroll` signs. A positive read is SPP state inclusion. Absence is non-inclusion in SPP's address tree under a root at or above the source's `absence_floor`.
- **Argument:** SPP lets only the owner of a data-bearing note create or spend it. "Included under the credential PDA" therefore means "created by `Enroll` after a valid `RegisterKey`", and no instruction ever spends it.
- **Code:** merged `programs/storage/src/instructions/enroll.rs`, `program-libs/interface/src/accounts/tracking.rs`, `prover/circuits/registry/register_key.go`. In progress `program-libs/interface/src/accounts/credential.rs`, `programs/custom-ring/src/instructions/enrollment/enroll.rs`, `prover/circuits/registry/credential.go`.

What exists now:

1. Merged: tracking `Enroll` with a verified `RegisterKey` proof, tracking reads in transfer, deposit and merge proofs, and an enrolled nullifier key that never changes. `ApplyEnrollments` has no verifying key, so no enrollment applies on chain yet.
2. Merged: the viewing-key resolver over zolana's `UserRecord` in both SDKs.
3. In progress, uncommitted: `AuditCredential`, the ring `Enroll` processor, the credential circuits, the absence floor and the private write proof. Core reports them implemented with integration pending, and no test or key has run.
4. On SDK lanes, unmerged: the credential resolver in both SDKs, and the transaction fit that counts credential trees.
5. The `RegisterKey` statement value is unchanged, so its key does not rotate. The compiled key has not been compared.

Open: the forester partial-batch flush that bounds activation latency is undecided. Transaction fit and CPI depth of `Enroll` are not checked.

### Zero-key notes

- **Status:** Partially.
- **Why:** Zero-key notes are merged. The agreed relaxation that lets an enrolled owner receive one is in progress.
- **How:** A zero-key note has nullifier secret 0, so its nullifier is public to anyone with the opening. Merged code requires the recipient to be not enrolled. In progress, a zero-key output reads no credential, and a salted list read still applies. Wallets mark such notes exposed if spent and never select them automatically.
- **Argument:** The auditor computes a zero-key nullifier from the public preimage, so linkability holds without enrollment.
- **Code:** `prover/circuits/transfer/owners.go`, `sdk-libs/client/src/state/exposed_notes.rs`, `sdk-libs/ts/src/state/exposed-notes.ts`.

## Delegate and approvals

### Permanent delegate

- **Status:** Partially.
- **Why:** The bypass rail is merged, but SPP has no verifying key for the delegate exit selector, and the Ring RPC request and the CLI carry no withdrawal or exit.
- **How:** `AuthorityTransfer` sets the delegate bit from the instruction tag. The circuit then holds the policy hash and the own and curator roots to zero and clears every screening and threshold gate. Nonzero-key inputs still need their owner's enrollment, outputs still meet the key rule and the audit stream still applies. A deposit leg is refused, and a withdrawal leg is accepted. An exit selects SPP's `RingAuthorityExit`.
- **Argument:** The program binds the policy roots as zero on this rail and the circuit asserts them zero. No prover choice can turn screening on, and no curator update can fail a delegate proof in flight.
- **Code:** `program-libs/interface/src/instructions/transact.rs`, `programs/custom-ring/src/instructions/transfer/accounts.rs`, `prover/circuits/transfer/transfer.go`, `sdk-libs/sdk/src/operations/delegate_source.rs`.

Superseded from PR #17: delegate moves no longer run list rules or guards, may withdraw, and select an exit rail.

### Co-signer approval

- **Status:** Implemented.
- **Why:** Classes, scopes and the demand check are merged.
- **How:** Five classes: `TRANSFERS`, `DEPOSITS`, `WITHDRAWALS`, `APPROVE_OUTFLOW`, `APPROVE_WITHDRAWAL`. Each co-signer has its own account and scope, and one may cover several classes. The first three are demanded only when the ring configured a co-signer for that class. A set approval bit always demands its class. The program refuses a co-signer that covers no class still uncovered, and one that is an owner signer or the in-proof owner. The circuit keeps the hidden owner apart from every co-signer.
- **Argument:** A set approval bit with no configured co-signer fails closed, so a crossed threshold fails without its approver.
- **Code:** `program-libs/interface/src/instructions/cosign.rs`, `programs/custom-ring/src/instructions/accounts/loader.rs`, `programs/custom-ring/src/instructions/configuration/access/set_cosigner.rs`.

### Co-signer limits

- **Status:** Implemented.
- **Why:** Ring RPC enforces per-asset window limits before it co-signs.
- **How:** Each asset record holds sender, volume, deposit and withdrawal caps, each with a window. Ring RPC reads the window and reserves the operation in one database transaction. It refuses an asset with no record and refuses when it cannot read the limits.
- **Argument:** The read and the reservation commit together, so concurrent retries cannot use one allowance twice.
- **Code:** `services/ring-rpc/src/operations/cosign_limits.rs`, `services/ring-rpc/src/storage/cosign_usage.rs`, `program-libs/policy/src/asset_limits.rs`.

Limits bind only the operations the ring requires co-signing for, and only when Ring RPC is the co-signer.

## Programs, accounts and roots

- **Status:** Partially.
- **Why:** The ring and Storage programs are merged. The credential switch that replaces the tracking accounts is in progress.
- **How:** One ring program per ring holds the roles, the mode, the auditor key, the delegate, the co-signer counts and the policy in one account. It pins its records and curator registries by address and reads their roots live by revision. No root is synced into the ring account. SPP's ring config at the ring PDA holds the activation, pause and delegate rail flags.
- **Argument:** A frame names revisions only and cannot supply a root value, so a revoked root fails in the next transaction.
- **Code:** `programs/custom-ring/src/lib.rs`, `programs/custom-ring/src/instructions/accounts/loader.rs`, `program-libs/interface/src/accounts/state.rs`, `programs/storage/src/lib.rs`.

Roles: admin, policy authority, pauser, co-signer admin and delegate, nonzero and distinct. The upgrade authority signs only `Initialize`. A write that touches a policy-readable list or mint settings applies with zero grace and retires every earlier root. Other writes keep the earlier root valid for the grace period. Pause stops deposits, transfers, withdrawals, merges and delegate moves. Record updates and controls still run. A frame larger than one transaction is submitted from a frame buffer by its SHA-256 digest.

## Ring RPC and SDKs

- **Status:** Implemented.
- **Why:** Relay, readers, history, managed merge and scanning are merged. Lanes for the input layout and the credentials are held for joint integration.
- **How:** Ring RPC checks each request against a quote that names its fee payer, proves, co-signs, pays and sends. The SDKs sign a returned message only after they rebuild and compare it. Readers use one grant store per deployment, the ring's record tree or a sealed database. History is a walk of finalized signatures, each event bound to its canonical transaction.
- **Argument:** The fee payer must equal the quoted key and differ from the signer, so a relay cannot make a wallet pay or sign a foreign instruction.
- **Code:** `services/ring-rpc/src/operations/relay.rs`, `services/ring-rpc/src/rpc/finalized_history.rs`, `services/ring-rpc/src/security/grants.rs`, `sdk-libs/sdk/src/validation/relay_message.rs`.
