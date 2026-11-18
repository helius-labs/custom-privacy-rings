# Storage program

## Instructions

| Tag | Instruction | Required signers | Behavior | Circuit proof |
|---:|---|---|---|---|
| 0 | `Initialize` | Creator and payer | Create a policy registry with its initial root, write authority and root-history settings. | None. |
| 1 | `Update` | Current registry authority | Apply a batch of policy-record writes and update the root history. Restrictive batches invalidate earlier roots; other batches preserve the configured grace period. | `ApplyRecordsCircuit` or `PrivateApplyRecordsCircuit`. |
| 4 | `UpdateAuthority` | Current and new registry authorities | Change the policy registry's write authority without changing its roots. | None. |
| 6 | `Enroll` | Owner and payer | Create a key-registration account with the escrow ciphertext and viewing key, or rotate an existing account's viewing key. Tree insertion is separate. | `RegisterKeyCircuit` on first registration; none for rotation. |
| 7 | `ApplyEnrollments` | None; permissionless | Insert pending registrations into the tracking tree and mark their accounts applied. | `ApplyEnrollmentsCircuit`. |
| 8 | `OwnerGrant` | Owner and payer | Create or update an owner-to-reader grant account for access through Ring RPC. | None. |
| 9 | `InitializeTracking` | Creator and payer | Create a tracking registry with its initial root, ring ID and auditor public key. | None. |

The current program has no separate `UpdateImmediate` instruction; `Update` uses the proof's restrictive flag to determine whether earlier roots remain valid.

Sources: [instruction dispatch](../programs/storage/src/lib.rs), [instruction tags](../program-libs/interface/src/instructions/storage.rs).
