# Co-signing and limits

A ring bounds what moves in three places. The ring program checks co-signer approval, the transfer proof enforces caps and sets approval bits, and Ring RPC enforces window limits and operator lists on the transactions that need its signature. [Spec](../spec.md)

## Who must co-sign

- A `CoSigner` record contains a signer and a scope over five classes. `TRANSFERS`, `DEPOSITS` and `WITHDRAWALS` are optional, and `APPROVE_OUTFLOW` and `APPROVE_WITHDRAWAL` are approvals.
- An operation demands a class from what it does. A note spend demands `TRANSFERS`, a deposit leg demands `DEPOSITS` and a withdrawal leg demands `WITHDRAWALS`.
- A frame with `no_spend` demands no `TRANSFERS`. A merge and a delegate transfer demand nothing.
- An optional class with no configured record demands nothing, so a ring with no co-signers settles those operations with none.
- An approval class stays demanded with or without a record. A proven bit with no signer of that scope is refused.
- Each signer must cover a class the earlier signers left. An unrelated record covers nothing and is refused.
- One record may cover several classes, and separate signers may cover separate classes.
- A co-signer differs from every owner in the run and from the in-proof owner. The party check refuses a hidden owner as its own approver.
- A co-signer signs the transaction, or approves one call of a program with `ApproveTransfer`, see [program owners](owner-rails.md#program-owners).
- Ring RPC co-signs only with its own key and only for classes its `approve` set names. An empty set refuses every operation but a delegate frame.

## Limits in the transfer proof

The limits bind on every ring with its own registry, with or without a policy hash, except on the delegate rail.

- `debit` is the amount of an asset in an owner's inputs, less what same-ring outputs return to that owner. It never falls below zero.
- `kept` is the rest of the owner's input amount. An owner's notes add up across nullifier keys, because rows key on owner identity.
- `leaving` is the exits of the asset plus its statement-bound gross withdrawal. An exit counts whoever receives it.
- `displaced` is what leaves beyond the debits, capped at the rows' input amount. It arises only when a deposit funds a same-owner return.
- `charge` splits `displaced` over the kept funds by running floors over the ascending rows. It conserves amounts, and no prover input selects a share.
- The charged outflow is `debit` plus `charge`, and the net outflow is `debit` alone. The peak charged outflow of each asset stays at or below its `cap`, and a zero cap turns the check off.
- A peak above `cosign_above` sets the `OUTFLOW` bit. A gross withdrawal above `withdrawal_threshold` sets the `WITHDRAWAL` bit, with no deposit subtracted.
- Neither threshold caps the charged outflow. Above one a transfer needs an approver's signature, and `cap` is the only bound on it.
- The withdrawal threshold reads the gross and no owner. It limits what a transaction withdraws in public, and `cap` limits what one owner's funds answer for.
- A zero threshold and an asset with no record set no threshold in the proof.
- The program demands the approval class of each proven bit, and refuses a bit on the delegate rail.

As an example, S spends 1000, deposits 1000, returns 1000 to S and exits 1000 to X. S has a net outflow of 0 and a charged outflow of 1000, so a cap of 100 refuses the transfer.

## Exact approval totals

- A co-signer that needs exact amounts opens one digest off chain. The digest covers each output asset's identifier and its peak and total charged outflow. It covers each public asset's identifier, its peak and total debit and its gross.
- The proof binds the digest over the values the outflow section computes, under a fresh blinder independent of the transaction key.
- A frame carries the digest only under exact approval, and a zero digest discloses nothing. Every key carries the digest constraints.
- The opening names no sender. Exact amounts and asset combinations can still identify a sender through outside information.

## Window limits, in Ring RPC

- Window limits are optional service policy and never a ring limit.
- Each asset's record holds a sender cap, a volume cap, a deposit cap and a withdrawal cap, each with its own window in slots. A zero cap is no cap.
- The sender cap counts one owner's outflow. The volume cap counts what the ring moves.
- Ring RPC checks them when it decides whether to co-sign, and the program checks only the co-signature.
- Ring RPC derives each row from the frame it signs. It refuses an input it cannot name with `LimitsUnavailable`, and it refuses an asset without a record.
- A co-signed message contains one ring instruction, and Ring RPC refuses a second relevant instruction. A program transfer's message instead holds the `ApproveTransfer` instructions and the one program instruction they name. Ring RPC also refuses a message that is not version 1.
- `owner_debits` in `program-libs/policy/src/quantities.rs` is the Rust computation. A shared vector pins it to the circuit.
- The window read and the reservation commit in one database transaction, so no allowance is granted twice.
- Admission reads one slot before its transaction. A slot behind the newest reservation of the same assets counts at that reservation's slot.
- Replicas with the same signing key share one store and reserve a signature once. A released signature cannot reserve again. Different signing keys cannot share one message allowance.
- Windows start at admission. Commit delay and transaction landing do not move the reservation's slot.
- A delegate frame reads no limit and reserves nothing.
- No proof carries a window limit. A proof of one would need authenticated usage state that every transaction updates.

## Operator lists, in Ring RPC

The lists are the centralized form of the ring's proven lists. The ring proves nothing about them, and the program checks only the co-signature.

- A ring turns them on with a `CoSigner` record for Ring RPC's key that covers `TRANSFERS`, `DEPOSITS` and `WITHDRAWALS`. The program then demands that signature on each of those operations.
- Each ring has an allow list, a block list and a freeze list. An entry names an owner, an asset or a destination. A destination is an account of a public withdrawal leg.
- A freeze refuses what a listed owner sends and every transfer of a listed asset. A frozen owner still receives.
- A block refuses a transfer that names the subject as sender, recipient, destination or asset.
- One allow entry closes its kind. Only allowed subjects of that kind pass, and the removal of the last entry opens the kind again.
- The checks run in the order freeze, block, allow. The operator's own rules run next, then the approval classes and the window limits.
- The ring policy authority signs `editCoSignList` and `getCoSignLists`. No other key reads or changes a list.
- Ring RPC lists an owner under the owner identity of its wallet address, the identity a frame names it with.

### What the co-signer reads

- Ring RPC opens every frame it co-signs with the ring's auditor key. It reads the recipients and the assets from the auditor envelope the transfer proof binds.
- It reads the senders from the input notes it has ingested. For a note it has not ingested, the signer run names the sender, and the lists decide with no wait.
- A frame that hides its sender or holds an in-proof owner has no such run. Ring RPC refuses it with `LimitsUnavailable` until it has ingested the notes.
- The delegate reads an owner's notes through `getDelegateNotes`. Ring RPC opens them with the auditor key too.
- The co-signer service and the delegate operator therefore read every amount, asset and owner of the ring. An operator that runs the lists cannot be kept blind to a transfer.

### A wallet that sends its own transaction

- `coSignTransaction` takes the wallet's message and returns Ring RPC's signature or a named refusal.
- Ring RPC simulates the message first. No co-signer rule verifies the ring proof, and the simulation runs the program's check of it.
- The same rules then run as on a relayed transfer.
- A program transfer adds `approvedTransfer`, the data and ordered accounts of the ring call. Ring RPC signs the `ApproveTransfer` only when its digest commits to that call and the simulation makes the call exactly once. It refuses a digest of another call with `-32164` and a call the simulation makes other than once with `-32176`.
- The message must name a blockhash the cluster holds, and its reservation records the last valid block height.
- Ring RPC learns the end from the finalized ring history. A published nullifier of the frame confirms the reservation. Once the block height passed the recorded height and the history covers a later finalized slot without those nullifiers, the reservation is released.
- One transfer holds one reservation. A second message with the same nullifiers and amounts takes it over, and another transfer of one of those nullifiers is refused until the reservation ends.
- A frame of public legs alone spends no note, so each proof of one holds a reservation until its blockhash expires.

### Limits of the lists

- A merge and a delegate transfer demand no co-signer, so no list gates them. A frozen owner can still merge its own notes.
- A relayed deposit is screened by its assets alone.
- A destination entry names an account. A new token account of the same wallet is another destination.
- A refusal names its list, so a sender learns that a party is listed.
- A change to a list binds the next signature. A signature Ring RPC already gave stays valid until its blockhash expires.
- Ring RPC must be online for every gated operation, and it can refuse any of them.

### Your own rule

A rule implements `CoSignRule`. It sees the ring, the message and the co-signature, and it reads what it needs from its own sources. A rule that reserves state keys it by the co-signature and frees it in `settle`. The relay settles a message it sends, and the rule settles a message the wallet sends in `sweep`.

## What passes them

- Window limits apply only to operations that demand a co-signer and that Ring RPC co-signs.
- The limits bind every operation of a class only when Ring RPC is the one co-signer that covers that class.
- A co-signer key outside Ring RPC whose scope covers the demanded classes signs with no window limit and no list.
- Each signing key is its own sender, so one person with many keys passes a sender cap once per key.
- Outflow caps and amount guards sum one transfer. A payment split across transfers passes each part.
- A deposit-funded payment to another ring owner, where nothing leaves, keeps the charged outflow at `debit`.
- The delegate rail runs no cap, no amount guard and no approval.

## Real-world controls

- A cap per transfer and asset applies on every rail but the delegate's.
- Dual control above an amount applies on owner transfers, and the transfer publishes the approval bits.
- A cap per customer per window applies only on operations Ring RPC co-signs.
- A cap per person across keys does not apply.
