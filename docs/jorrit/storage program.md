# Storage program

## Instructions

| Tag | Instruction | Required signers | Behavior | Circuit proof |
|---:|---|---|---|---|
| 0 | `Initialize` | Creator and payer | Create a policy registry with its initial root, write authority and root-history settings. | None. |
| 1 | `Update` | Current registry authority | Apply a batch of policy-record writes and update the root history. Restrictive batches invalidate earlier roots; other batches preserve the configured grace period. | `ApplyRecordsCircuit` or `PrivateApplyRecordsCircuit`. |
| 4 | `UpdateAuthority` | Current and new registry authorities | Change the policy registry's write authority without changing its roots. | None. |
| 7 | `ApplyEnrollments` | Registry authority; each write carries its owner's signature, or a PDA owner's CPI signature | Insert key registrations with their escrow ciphertext and viewing key into the tracking tree. A later write rotates the viewing key. There is no separate `Enroll` instruction or registration account. | `ApplyEnrollmentsCircuit`, plus `RegisterKeyCircuit` per first registration. |
| 8 | `OwnerGrant` | Owner and payer | Create or update an owner-to-reader grant account for access through Ring RPC. | None. |
| 9 | `InitializeTracking` | Ring PDA, through CPI from the ring's `Initialize`, and payer | Create a tracking registry with its initial root, ring ID and auditor public key. | None. |
| 10 | `EmitEvent` | None; valid only as the storage program's own inner instruction of `ApplyEnrollments` | Publish the applied enrollments for indexers. | None. |

The current program has no separate `UpdateImmediate` instruction; `Update` uses the proof's restrictive flag to determine whether earlier roots remain valid.

Sources: [instruction dispatch](../../programs/storage/src/lib.rs), [instruction tags](../../program-libs/interface/src/instructions/storage.rs).
