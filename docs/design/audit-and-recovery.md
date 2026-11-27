# Audit and recovery

The auditor holds one viewing key for the ring's life. Every operation proves a record per output, sealed to a recipient key the auditor reads. Payloads and non-exit tags stay outside every proof. [Spec](../spec.md)

## Transfers

- The sender draws a transaction viewing key. The transfer proof seals its secret to the auditor in one AES-256-CTR stream.
- The key schedule binds the auditor key, the transaction key and a salt. A resend repeats the key, and a fresh salt gives a different keystream.
- The stream contains an envelope and one recipient key per published slot.
- On a vouched transfer the stream also contains the Ed25519 key that vouched. The proof seals the key whose signature it verified, so the auditor reads who answers for every sender and owner of that transfer.
- Each slot has one record, sealed from the transaction viewing secret to that slot's recipient key. A non-exit slot's body carries it, and an exit's travels in the auditor message. The recipient and the auditor read the same record.
- A record names the note's kind, amount, owner key, mint and blinding, an exit's nullifier key and the data hashes its frame's layout names. [Records](../spec/crypto/disclosure.md#records) fixes the bytes. A dummy's record is all zero, and the auditor rebuilds the dummy hash from its blinding.
- The proof holds every published blinding to a derivation from the sealed key, the salt, the first nullifier and the slot index.
- The auditor rebuilds each commitment from its record and the tree id, with zero for a hash the layout leaves out. It needs no other slot, no history and no mint registry.
- A record of another slot rebuilds no published hash, so the reader refuses it.
- The record proves `data_hash` and `ring_data_hash`. A clear frame bit proves that hash zero on every slot. A lost payload loses the application data and not the commitment.
- The record chain tags each cell with the slot's exit claim, so a frame cannot present a ring record as an exit's.
- The program builds the ring statement from the frame it forwards to SPP, so the proof binds the transfer that settles.

## Spent notes

- A ring input takes no record. The envelope seals `R`, the count of spent notes, and no note position, see [envelope plaintext](../spec/crypto/disclosure.md#envelope-plaintext).
- The ring's inputs hold SPP's spent notes in note order, then dummies. SPP's address chain reaches the proof as a private witness alone.
- The auditor recomputes a nullifier from an opening it already read and the owner's sealed secret. It then matches SPP's published list, and a match links the spend.
- No constraint binds a nullifier to a record. A note the auditor never opened stays unlinked until the opening arrives. `R` tells the auditor how many of the published nullifiers are spent notes.
- A default-ring input has no earlier record. An entry key therefore names it in the stream at its note position, with its owner key, nullifier key, amount, mint and spent commitment.
- An entry record discloses no blinding, data hash or secret, and one frame can carry default-ring inputs of several owners.

## Exits and withdrawals

- An exit's record has its own kind and opens the created default-ring note for the auditor. The SDKs seal it to the auditor's key, and it travels in the auditor message.
- An exit's body is SPP's confidential output form, in the same layout as a default-ring transfer. No ring proof binds it.
- A withdrawal adds no stream bytes, since SPP publishes its asset, amount and destination.

## Nullifier secrets

- A wallet has one nullifier secret. Enrollment seals it to the ring's auditor key beside the wallet's list salt, and the program verifies the seal proof, see [tracking](records-and-roots.md#tracking).
- The `EnrollmentsApplied` event carries the ciphertext, and the tracking leaf holds its hash. A restarted auditor checks each row the ring indexer serves against the tracking root and each seal against the row's `ct_hash`, then opens the seal.
- A tracking scan that has not reached the root's next index is no evidence that an owner is unenrolled.
- With a secret the auditor derives the nullifier of any note whose hash and blinding it knows. The derivation reaches every ring and the default pool.
- One leaked secret therefore exposes that wallet's spends across every ring it enrolled on. A wallet that wants separation uses another keypair per ring.
- A note under the zero nullifier key carries no secret. Its sender and the auditor both compute its nullifier, see [zero-key notes](owner-rails.md#zero-key-notes).

## Recipient delivery

- The proof makes each record's encryption to its supplied recipient key correct. It does not tie that key to the intended recipient, and choosing it is the sender's responsibility.
- Discovery tags, payloads and replies stay outside the circuit. SPP's external data hash binds the bytes a frame submits. A replaced byte fails SPP's verification, and a correct proof says nothing about what those bytes contain.
- A transaction with a correct proof can therefore carry a wrong key, a payload or a tag its recipient cannot use. Nothing promises delivery.
- A full scan repairs a bad discovery tag. It never repairs a record sealed to another key, since no held key rebuilds that note's commitment. A malformed payload loses the application data alone.
- Auditor recovery is a separate authorized fallback. It depends on Ring RPC, and an owner with no Ring RPC cannot open a note sealed to another key.

A body is the record, then under `HAS_DATA` a payload with the application data alone, see [slot body](../spec/crypto/disclosure.md#slot-body). The cipher has no authentication, so the commitment a reader rebuilds under its own keys is the only proof of ownership.

The sender's SDK pays a nonzero key to any owner, enrolled or not, since only the owner's enrolled key spends the note. It refuses the literal zero key, and the zero key for an owner with a tracking row under the root it verified. For an enrolled owner it checks the address's viewing key against the viewing key hash of the owner's row and refuses an address that names another. That check protects an honest sender and binds nobody else.

## Auditor fallback

- `getWalletStatus` returns each of the wallet's notes with its opening, read from the proven record. A wallet whose record was sealed to another key rebuilds its note from that opening.
- A wallet authorizes the read with its signature. A program cannot sign off chain, so it writes an `OwnerGrant` account through CPI that names a reader key.
- The fallback covers notes the service opened.

## Wallet scanning

- A sync asks the indexer for exact tags, and it reads ring history only in a recovery scan. One tag read serves both ring modes. Rust syncs through `RingWalletSync`, and TypeScript through `syncRingWallet`.
- Per retained viewing key the wallet stores a `tx` and a `request` counter and one tag position. A recovery scan adds one position per ring. A counter's `next` is zolana's `tx_count` or `request_count`, and its `scanned` is one past the highest index a sync found.
- For each held key the wallet lists the bootstrap tag, and the sender and request streams from index 0. A stream's list ends `TAG_WINDOW = 64` indices past its `next`. zolana's First Time Sync Wallet sets that rule and names no window size. 64 is `DEFAULT_TAG_WINDOW` of zolana's test wallet.
- The key holder derives each listed tag and each body's slot secret. No viewing key and no raw agreement leaves the holder.
- One indexer call names at most `TAGS_PER_REQUEST = 1000` tags, so a longer list takes more calls.
- The wallet reads the bootstrap tag and stream indices below `readEnd` from the saved tag position. New indices start at the first indexed block. A saved counter without `readEnd` starts there once.
- A returned transaction moves a stream counter past a listed tag only at a slot that opens under the key that lists the tag, whatever its ring. A slot opens when its body decrypts and its commitment rebuilds under that key. A tag proves no ownership, and a transaction that opens no slot moves no counter.
- The wallet lists again after a round that moves a `next` counter, and the sync ends at a round that lists no tag. It sets the tag position of every held key to the lowest final slot of its reads.
- The wallet tries each ring body of a returned transaction under every held key, with no use of its tag. A note counts only when its commitment rebuilds.
- The wallet then reads the spends of its unspent notes by nullifier and opens those transactions the same way. That read finds change under random bytes and a delegate's move.
- A reply on a body the wallet rebuilt joins its pending claims, unauthenticated. The nullifier read collects no reply.
- A deposit is found through its tag stream. A merge is found from its nullifiers, and the ring proof binds the ciphertext the wallet opens.
- Full recovery reads a ring from its first row and tries every ring body without a tag. Rust uses `AnonymousSync::new(keys, assets).with_full_scan(ring).fetch_wallet(indexer, wallet)`. TypeScript uses `fullScanWallet(client, wallet, ring)`.
- The recovery scan reads each ring the wallet watches from that ring's stored position, and tries every ring body without a tag. It opens a note only when its body is sealed to a key the wallet holds. Rust runs it ahead of the tag read with `RingWalletSync::with_recovery_scan`, and the CLI with `balance --recover`.
- A body a full scan leaves unopened stays in `auditor_recovery.unopened`, foreign notes and dummies among them.
- A wallet rebuilds a ring note under its own nullifier key, or under the zero key the record's bit names, and reads no registry. A note under the wallet's key opens before that key is enrolled, and it spends only once the key is enrolled.
- A zero-key note enters the wallet as exposed if spent. No automatic path selects it.
- Only a final page's `context.slot` becomes a stored position. A failed page moves nothing, and a key added later is its own query.
- `AnonymousWallet` contains notes, pending notes, spends, reply claims and positions in one document. `absorb` takes a scan's results together, so a position is never saved without what it covers.
- A body under an unknown mint can stay pending until mint resolution.
- `RotatingRingKeys` contains one identity's current and retired viewing keys, so a note sealed to any of them opens.
- The CLI refuses a wallet store that contains no entry of the wallet's viewing key.

Durable recovery fixes its target slot and stores its continuation with the wallet. `Paused` means coverage is incomplete. Absorb each returned scan, then repeat the same durable call and scan scope. Rust full scans keep `with_full_scan(ring)`. TypeScript full scans repeat `fullScanWallet` with the same ring.

An unspent note whose nullifier check has not reached the target reads `PendingSpendCheck` in Rust or `pendingSpendCheck` in TypeScript. Wallet balances and exposed-note lists omit it. `notes` remains the full inventory.

TypeScript sets its target at or above the saved recovery floor, ring and tag positions, core checkpoints, private-transaction slots and configured `requireSlot`. The recovery floor retains the highest committed target or authenticated note observation. Imported notes without publication slots supply no additional floor.

History-only calls do not preserve a wallet recovery session. A paused Rust `fetch` or `full_scan` result must be absorbed and resumed through `fetch_wallet`. TypeScript refuses to absorb a paused history-only scan with `RING_SYNC_INCOMPLETE`, and `fetchWallet` or `fullScanWallet` resumes it.

### Tags on a transfer

- A wallet's history gives one anonymous transfer its sender tag, in Rust with `ViewingHistory::select`. The tag takes the index `tx.next` of the current viewing key, and the selection returns the history with the counter past that index. The wallet stores that history before it builds the transfer, so two transfers never take one index from one store.
- Each payment takes its request's tag, or with no request its recipient's bootstrap tag. A payment to the wallet's current viewing key takes the sender tag. The SDKs publish each tag of an anonymous frame once, see [Discovery](../spec/crypto/disclosure.md#discovery).
- A selection whose transfer never lands leaves its index unused. A sync still reads every index, since it lists each stream from index 0 to a window past its counter.
- A spend of ring notes builds without the sender tag. Each payment takes its request's tag, or with no request its recipient's bootstrap tag. Each change slot takes random bytes, and the ring sync follows the spent ring note by its nullifier to the change.
- An entry spend, default-ring notes and no ring note, is refused without the sender tag. A core sync that reads the spend of a default-ring note ahead of the ring sync hands that transaction to no later sync, so the sender tag is the one lead to the change short of a recovery scan.
- A deposit to another member's request builds without the sender tag, it returns no change. A deposit to the depositor's own address with no request is refused without it, its note would publish the depositor's bootstrap tag beside the funder.
- A recipient mints a request tag at its `request.next` and stores the moved counter. The wallet refuses a mint only at the `u64` limit and while it is in a recovery scan. No count of unpaid requests refuses one. The CLI mints one with `address`, and `address --bootstrap` prints the bare address with no tag.

### Limits of the tag read

- The indexer receives every tag the wallet lists and learns that wallet's transfers as they land, see [who else sees what](privacy.md#who-else-sees-what).
- The list holds one tag for each request a key ever minted and each transfer it ever tagged, and 64 more per stream. Every sync sends the whole list, at most `TAGS_PER_REQUEST` tags a call.
- A wrong tag hides a payment from the sync, unless another read of the sync returns its transaction. The recovery scan finds it when its body is sealed to a held key. A note sealed to another key opens under neither.
- Tags repeat across transactions, and zolana's View Tags say a tag should not be reused. The bootstrap tag repeats on every payment with no request. A request tag repeats when its request is paid twice, and a sender tag when two transfers take one index. The SDKs publish each tag once inside one anonymous frame.
- Two devices on one seed keep separate counters and take the same indices. Equal sender tags then link their transfers in public, and both devices mint the same request tags. A sync finds every transfer under a repeated tag.
- A wallet restored from its seed alone starts each counter at zero. Its sync stops at the first 64 unpaid requests in a row and finds no later one. `ViewingHistory::with_request_count` states how many requests the key minted, and the list then covers them. The CLI states that count with `address --request-count <n>`. The recovery scan finds the notes without it.
- A member's bootstrap tag is public, so anyone can publish a slot under it. The wallet pays the trial opens of that transaction, each ring body under each held key.
- Exact-tag sync verifies each opened ring event against a finalized Solana transaction before it stores a note, reply or tag counter. A missing transaction or a different event stops the tag read. The indexer can omit events. Full-history scans and nullifier reads still depend on the indexer.
- A transfer with the sender tag needs the wallet's stored counters, since the tag takes its index from them. A wallet that sends from stale counters repeats an index, as two devices do.

## Deposits and merges

- A deposit is a transfer that spends no note, and its outputs open through the auditor stream.
- A merge seals one record of the output, followed by the seed, to the owner's viewing key. An envelope seals the ephemeral key and that viewing key to the auditor. The auditor opens the envelope, then the record, and rebuilds the published hash from it.
- SPP's proof ties every merge input to that owner, asset and sum. The ring proof therefore opens no inputs, and one ring key serves every width.
- SPP's merge publishes no auditor message, and no external data hash covers the record. The auditor reads both ciphertexts from the ring instruction, and the statement alone binds those bytes.
- The merge's seed also classifies its padding nullifiers. Its real nullifiers mark inputs through the note index.

## Reading through Ring RPC

- Ring RPC enumerates finalized signatures and binds each indexed event to the canonical transaction.
- Missing or altered indexed events stop the page before coverage advances.
- Each published nullifier takes one state. A transfer names no note position, so each of its nullifiers reads as a spend until the note index resolves it. A merge's seed classifies its padding.
- A nullifier with no hit stays unclassified, whether it is an unopened note, an address, a default-ring note or padding. It never reads as padding or unspent, and a later opening resolves it.
- One database transaction per finalized page commits the rows, the notes and the position.
- `getWalletStatus` derives each note's status from that note's own nullifier. An unresolved spend elsewhere never changes it, and the response names no other transaction.
- Only an unreadable stream stops `history_through` for every wallet. The balance sums `Unspent` notes, and `Unknown` and `ExposedIfSpent` stay out of it.
- `getSpendCoverage` is the one read that lists unclassified nullifiers, and it serves the ring authority alone.
- `getDelegateNotes` hands the owner's openings and nullifier secret to the ring's configured delegate alone. No other read returns a nullifier secret.
- A co-signer reads debits from the frame and the note index. It refuses with `LimitsUnavailable` a frame whose resolved notes and entry records do not count `R`.

Member recovery returns a bounded sample of unmatched input nullifiers and their total count. The sample does not establish complete spend coverage.
