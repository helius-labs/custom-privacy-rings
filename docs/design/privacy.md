# Privacy by mode

The confidential mode and the anonymous mode differ at the sender and the recipients. Amounts, assets and blindings stay private from chain observers in both. [Spec](../spec.md)

## Confidential rings

- Chain observers see the sender, in the owner run or as the in-proof owner. An authority transfer shows the delegate in its place.
- Each slot publishes the last 8 bytes of its recipient's viewing key as its tag. Payments to one recipient therefore link to each other, and a dictionary of known viewing keys names that recipient.
- They see each exit owner and each public leg's account, asset and amount.
- They do not see amounts, assets, blindings or which notes a transfer spends.
- A confidential deposit's tag is the recipient's viewing key. The tag links the funder to the recipient.

## Anonymous rings

- Chain observers see the payer, the published input count, the published output count and 8 tag bytes on each non-exit slot. [Discovery](../spec/crypto/disclosure.md#discovery) fixes each slot's tag.
- A payment with no request publishes the low 8 bytes of its recipient's bootstrap tag, the recipient's viewing key. Transfers that pay one recipient under it link to each other. Enrollment publishes each enrolled owner's viewing key, so every observer reads which enrolled member each such transfer pays. The sender, the amount and the asset stay hidden.
- No payment pairs a sender with its recipient, so each later payment with no request publishes the bootstrap tag as the first does.
- A payment to a payment request publishes the request's tag in place of the bootstrap tag. Whoever read the request links the payment to its recipient. Transfers that pay one request publish the same tag, so every observer links them to each other.
- The SDKs publish each tag once in a frame, and a slot that would repeat one takes random bytes. The tags of one transfer therefore do not show that two of its slots share an owner.
- A public co-owner signs in the open. Its link to the transfer is public, and the hidden owner's stays hidden.
- Under `HAS_DATA` the program requires one payload width for every non-exit slot of the frame, dummies included. Data can increase that width, and observers see whether a frame carries payloads.
- A reply has one length per frame, and a slot with no reply carries random bytes of that length. Observers see whether a frame carries reply bytes.
- They see each exit owner and each public leg.
- They do not see the hidden sender, the amounts, the assets or the spent notes. They do not see a recipient whose payment takes a request tag.
- The hiding holds only while all of these hold:
  1. The payer, each funder and each other signer do not link to the owner.
  2. The transfer spends ring notes only, none under the zero key. A default-ring spend publishes its owner, and a zero-key spend names it to anyone who holds the note's opening.
  3. No exit or withdrawal pays a key linked to the owner.
  4. An anonymous deposit takes a sender or request tag. The SDK refuses a deposit note under its recipient's bootstrap tag, since a deposit publishes its funder beside that tag.
  5. The wallet trusts its indexer, which pairs each spent leaf with its nullifier and receives every tag the wallet lists.
  6. The prover runs locally or in an attested enclave.
  7. The rules admit many owners. An allow rule bounds every sender and recipient to its list.
  8. Deposit and withdrawal amounts and timing do not single out a later spend.
- No proof holds a tag to any form. A sender can repeat a tag or encode data in one, and two slots under equal tags link to each other.
- A published `ring_data_hash` of an application value can link notes. No proof holds it to a salted commitment.

## Every mode

- Enrollment is public. An `EnrollBatch` publishes each owner, its nullifier key, list tag and viewing key in instruction data. Anyone sees that a wallet enrolled on a ring.
- A tracking row commits the owner's viewing key for one ring. A leaked viewing secret opens the bodies sealed to that key on every ring whose row commits it. A rotation on one ring moves that ring alone to a new key.
- A transfer, deposit or merge publishes the tracking revision it reads. The revision shows that each row the proof opens existed at that revision, and names no owner.
- A private write raises the source's tracking floor to the revision its rows open under. The floor shows that each written owner was enrolled by that revision or absent at it.
- A public record write names its member. A rule table is public, so observers see which lists a ring screens and who is on the public ones.
- A deposit publishes the funder, the asset and each amount.
- A merge links its input notes to one owner.
- A merge record carries the owner's nullifier secret. The auditor key and the owner's viewing key each open it. A viewing key rotation does not take back a secret already read.

## Private lists

- A private entry is an ordinary record under a member key salted per owner. The key is pseudorandom to anyone without that owner's salt.
- List bits on a salted record stay visible with no identity, so per-list counts and update timing are public.
- A holder of an owner's salt links the owner to its record and reads the record's list bits. A published ring address contains the salt, so every reader of the address can make that link.
- The salt is final, so a disclosed salt reveals status permanently.
- An unenrolled identity has no salt of its own. A ring that commits a ring salt lists it privately under that salt, and a ring that commits none lists it only on a public list.
- Any holder of the ring salt can compute a known identity's ring-salted key and test that identity for a private entry.
- A transaction can read a salted list. The party that proves the transaction holds the witness and learns the salts and list status of the other enrolled owners in that proof.
- On a ring that commits a ring salt every proof that reads a salted list takes the ring salt, so every party that proves such a transfer holds it. Ring RPC returns it through `getListSalt` to list writers only.

## Who else sees what

- The auditor opens every output record and merge record. With an owner's sealed secret it links that owner's spends, see [nullifier secrets](audit-and-recovery.md#nullifier-secrets).
- The auditor reads each owner's list salt from the enrollment ciphertext. A curator learns the salt of each owner it writes for, and the ring salt when it writes for an unenrolled owner.
- A zero-key note's sender computes its nullifier and sees when the note is spent.
- By default the recipient of a CLI `transfer` on an anonymous ring learns the sender's viewing key, owner key and nullifier key from the reply. Enrollment publishes that viewing key with its owner, so the recipient learns which member paid, as an unauthenticated claim. `transfer --no-reply` leaves the reply out, and the CLI leaves it out of a transfer that does not fit with it.
- A reader sees what its grant covers, blindings included, and keeps it after a revocation.
- Ring RPC as relayer receives every secret the proofs need. It links the owner's spends and opens the outputs, so a relayed transfer is anonymous only toward a trusted Ring RPC.
- Ring RPC as co-signer sees each owner's charged outflow of each asset. It reads them from the frame with the auditor key.
- An outside co-signer that reads exact totals sees each asset's totals and no sender list.
- The ring indexer sees which members a client opens, before the transfer lands.
- Photon sees which leaf each nullifier spends. A syncing wallet sends it the bootstrap tag and the listed sender and request tags of each held viewing key. Photon therefore links those tags to one wallet and learns that wallet's transfers as they land. It also sees which nullifiers a wallet follows, and which ring history a recovery scan reads and from which slot.
