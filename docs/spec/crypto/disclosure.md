# Disclosure

A ring transfer opens through one proven record per output slot. Each record is sealed to the recipient key the sender supplies for its slot. One AES-256-CTR stream, the auditor stream, seals the transaction viewing secret, the note count and every slot's recipient key to the auditor. Payloads, non-exit tags and replies stay outside every proof. [Spec](../../spec.md) [Envelope](envelope.md)

## Verified audit recovery

Every published slot of a transfer whose proof verifies has one record the auditor opens.

- A ring output, an exit and a dummy each have one record. An exit's record opens the created default-ring note, its owner key included.
- The auditor reads each slot's recipient key from its own stream. A key the sender chose wrongly changes what the recipient can open and leaves the auditor's reading unchanged.
- The record names the owner key, amount, mint and blinding, an exit's nullifier key, and the data hashes the frame's flags name. The proof holds each hash to the note, so a malformed payload loses the application data and not the commitment.
- A spent ring note has no record. The auditor links its nullifier from an opening it already holds and the owner's enrolled secret. A note the auditor never opened stays unlinked until its opening arrives, and it withholds no output slot.
- A spent default-ring note has an entry record in the envelope of an entry key. It names the owner, nullifier key, amount, mint and spent commitment, and discloses no blinding, data hash or secret.
- Ring RPC serves an owner its opening from the record, see [auditor fallback](../../design/audit-and-recovery.md#auditor-fallback).

## Discovery

Each published slot carries an owner tag. A non-exit slot's tag is an unproved hint. No proof constrains it, and SPP's external data hash binds only its bytes. A frame carries the low 8 bytes of a non-exit tag, and the program restores the 24 leading zero bytes before SPP reads it.

| Slot | Tag |
|---|---|
| Ring output, confidential frame | low 8 bytes of the recipient's `viewing_u` |
| Dummy, confidential frame | low 8 bytes of the slot's [dummy recipient key](hashing.md#slot-secret) |
| Payment to a payment request, anonymous frame | low 8 bytes of the request's `recipient_request_view_tag` |
| Payment with no request, anonymous frame | low 8 bytes of the recipient's `recipient_bootstrap_view_tag`, its `viewing_u` |
| Payment to the paying wallet's current viewing key, anonymous frame | low 8 bytes of the wallet's `sender_view_tag(tx_count)` |
| First change slot, anonymous frame | low 8 bytes of the paying wallet's `sender_view_tag(tx_count)` |
| Dummy, anonymous frame | 8 random bytes |
| Exit | SPP's 32-byte owner tag, which SPP's proof holds to the exit's owner |

`sender_view_tag`, `recipient_request_view_tag` and `recipient_bootstrap_view_tag` are zolana's [View Tags](../../../zolana/docs/spec.md#view-tags), and the ring adds no derivation. `tx_count` is the paying wallet's sender index under its current viewing key. No slot takes zolana's `recipient_shared_view_tag`, and no record names its sender, so no payment pairs a sender with its recipient.

The SDKs publish each tag of an anonymous frame once. A slot whose tag an earlier slot of the frame publishes takes 8 random bytes in its place. Such slots are a second change slot, a second payment under the same tag and a note the wallet pays itself beside its change. The indexer returns every slot of a transaction for one matching tag, so one slot leads the reader to all of them. Equal tags would show an observer that two slots share an owner. Across transactions a tag repeats, see [limits of the tag read](../../design/audit-and-recovery.md#limits-of-the-tag-read).

The paying wallet chooses every non-exit tag, and a delegate's request chooses the tag of each payment it moves. A transfer built without its wallet's sender tag publishes each payment's request tag, or with no request its recipient's bootstrap tag. Each of its change slots publishes random bytes. The SDKs build such a transfer only when it spends a ring note. A slot that returns change to a key whose tags the builder does not hold publishes random bytes. A deposit's fresh sender and the sources of a delegate move are such keys. A delegate's payment to the source whose change leads the frame sits among that change, and one change slot publishes its tag.

A tag only selects a candidate, and the rebuilt commitment proves ownership. A frame therefore spends 8 bytes on each tag under the 4,096-byte transaction ceiling. SPP and the indexer hold a non-exit tag as 24 zero bytes and its 8 tag bytes. A reader asks for that form, and a query for a full 32-byte zolana tag matches no ring slot. Two unrelated tags can agree in 8 bytes. Such a match costs the reader the trial opens of that transaction, each ring body under each held key.

A payment request is the text `zolana:<address>?amount=&mint=&label=&tag=`, with `<address>` the recipient's ring address in base58. `tag` is 32 bytes in base58, and the paying wallet publishes its low 8 bytes. The SDK mints it as 24 zero bytes and the low 8 bytes of the recipient's request tag. The query part is unauthenticated.

A wrong tag hides a note from its recipient's sync, unless another read of that sync returns its transaction. A full scan tries every body without a tag and finds a note sealed to a key the reader holds. [Wallet scanning](../../design/audit-and-recovery.md#wallet-scanning) states how a reader finds its notes.

## Auditor message

A frame's SPP messages are its application messages in frame order, then the auditor message. An application message is a view tag and data the frame hands to SPP unchanged. SPP counts its messages in one byte, so a frame carries at most `MAX_APPLICATION_MESSAGES = 254` application messages. The auditor message is SPP's last message, under the auditor's `u` as its view tag. Its data is:

```text
0x04 || tx_viewing_u || envelope_ct || keys_ct || exit_record_ct of each exit in slot order
```

`tx_viewing_u` is SPP's `tx_viewing_pk` without its tag. `keys_ct` holds 32 bytes per published slot. Each exit record ciphertext is the exit's [published prefix](#records), 137 bytes or 169 under `HAS_DATA`.

`n` is the published input count, the frame's nullifier count. It is never the key capacity and never a private live count. The entry flag, the `vouched` flag, `HAS_DATA`, `n`, `m` and the exit bitmap fix the auditor message's length. The frame's flags, shape and counts fix every other length, and the program refuses trailing bytes.

## Stream regions

The stream has one key, one nonce and one `CTREncrypt` call in the circuit over the capacity-sized plaintext. Each region is zero padded to a multiple of 16 bytes, so it starts on its own counter and no counter serves two regions. The key alone fixes every counter, so a reader decrypts the key of slot `i` with no other slot. The frame carries only transmitted bytes, and padding and unpublished slots never leave the prover.

| Region | Start counter | Maximum blocks | Transmitted bytes |
|---|---|---|---|
| Envelope | `2` | `E` | `33`, or `41 + 136 n` on an entry key |
| Recipient key of slot `i` | `K0 + 2 i`, `K0 = 2 + E` | `2` | `32` for `i < m` |
| Voucher key | `K0 + 2 N` | `2` | `32` on a vouched frame |

`E = ceil(33 / 16) = 3` on a key without entry capacity, and `E = ceil((41 + 136 C) / 16)` on an entry key of input class `C`. `N` is the key's output capacity, so the voucher region moves no recipient key's counter.

The voucher region holds the Ed25519 key that vouched for the transfer, and zeros on a transfer nobody vouches for. A vouched frame transmits its 32 bytes after the `m` recipient keys. The statement binds `envelope_digest = Poseidon(digest(envelope_ct), digest(keys_ct), hash_bytes(voucher_ct))`, with zero in the last place on a frame that is not vouched.

## Envelope plaintext

```text
tx_viewing_sk || u8(R)                                                             key without entry capacity
tx_viewing_sk || u8(R) || u64be(default_note_bits) || entry_record * n              entry key
entry_record = owner_key || fe32(nullifier_pk) || u64be(amount) || mint || fe32(input_commitment)
key_i        = V_i
```

`R` counts the spent notes among the published inputs, with `1 <= n <= C <= 64` and `R <= n`. The ring's input slots hold those notes in SPP's note order, then dummies, and no address enters a ring input slot. The circuit takes SPP's address chain as a private witness and checks it only through `private_tx_hash`. No statement element carries a nullifier but the first, so the proof names no note position among the published nullifiers. The hidden rail holds ring input `0` to a note.

Bit `k` of `default_note_bits` is set exactly when note `k` is a default-ring note. `entry_record_k` names that note in 136 bytes and is all zero otherwise, every `k >= R` included. Records follow note order, never the nullifier position. `mint` is the 32-byte mint address, and the proof holds the note's asset to it. `input_commitment` is the note's spent UTXO hash, the value `private_tx_hash` chains, and the proof holds the owner key, nullifier key, amount and mint to it.

## Spent notes

- A key without entry capacity refuses a default-ring input, since no evidence would open it.
- The auditor recomputes SPP's nullifier of a note from the opening it already read and the owner's secret from enrollment. Under the zero nullifier key the secret is zero. A match in SPP's published list links the spend.
- No constraint binds a nullifier to an opening. Ring RPC reads every published nullifier of a transfer as a spend until its note index resolves it. A nullifier no opened note owns stays unclassified, whether it is an unopened note, an address, a default-ring note or padding. It never reads as padding.
- `R` is the completeness check. The co-signer counts the linked notes and the entry records and refuses a frame whose count differs from `R`.
- An entry record discloses its owner key, nullifier key, amount, mint and spent commitment, and no blinding, data hash or nullifier secret. The auditor reads the commitment and does not rebuild it. Each record carries its own owner and key, so one frame spends default-ring notes of several owners.

## Records

```text
head   = u8(kind) || u64be(amount) || owner_key || mint || fe32(blinding)     bytes 0 .. 105
record = head || cell_1 || cell_2                                              169 bytes sealed
```

| Slot | `cell_1`, bytes 105 .. 137 | `cell_2`, bytes 137 .. 169 | Published prefix |
|---|---|---|---|
| Exit | `fe32(nullifier_pk)` | `data_hash` | `137 + 32 DATA` |
| Ring output, dummy | `data_hash` under `DATA`, else `ring_data_hash` | `ring_data_hash` | `105 + 32 (DATA + RING_DATA)` |

`DATA` and `RING_DATA` are the frame's `HAS_DATA` and `RING_DATA` flags, each `0` or `1`. Every slot seals the 169 bytes, and the frame carries the published prefix alone. A ring output therefore publishes the head, then `data_hash` under `DATA`, then `ring_data_hash` under `RING_DATA`. An exit publishes the head, its nullifier key, then `data_hash` under `DATA`.

| `kind` | Slot | Plaintext |
|---|---|---|
| `0` | dummy | all zero |
| `1` | ring output | the slot's opening |
| `2` | exit | the slot's opening |

Bit value 4 of the kind byte is set exactly when a live slot's nullifier key is the zero key. A ring output's record carries no nullifier key. A recipient takes the zero key under that bit and otherwise rebuilds under its own key with no registry lookup. The auditor reads a nonzero key from the owner's enrolled row. The proof derives the bit from the key the commitment hashes.

The proof holds every slot `i < N` to the UTXO or the dummy domain, dummies last. A dummy's owner, asset, amount, data hash, ring data hash and ring id are zero, and SPP derives its blinding. No real output sits at or past `m`. The proof derives `kind` from the domain and ring id the slot's hash commits, so no prover choice passes a note as a dummy. `owner_key` is the raw key whose `solana_owner_identity` the commitment hashes, and `mint` the raw mint whose `hash_bytes` is its asset. The auditor names an owner and an asset with no directory. `blinding` is the slot's blinding, which `private_tx_hash` and SPP's derivation fix. The proof holds `data_hash` and `ring_data_hash` to the slot's note. A clear flag holds that hash to zero on every slot, and an exit holds no ring data hash.

The statement chains one cell per published record, `Poseidon(exit_i, hash_bytes(record_ct_i))`, see [public inputs](public-inputs.md#transfer). `exit_i` is `1` on a slot the frame's exit bitmap names, so a ring record never reads as an exit's at the same length.

Each record is sealed from the transaction viewing secret to the slot's recipient key `V_i`:

```text
dh_i        = X25519(tx_viewing_sk, V_i)          V_i canonical, dh_i nonzero
record_ct_i = CRING/rcp stream under CRDS_i from counter 2, the published prefix of the plaintext
```

| Slot | `V_i` |
|---|---|
| Ring output | the recipient's viewing `u`, the sender's own on change |
| Exit | any valid key, the SDKs supply the auditor's `u` |
| Dummy | the slot's [dummy recipient key](hashing.md#slot-secret) |
| Unpublished capacity slot | prover only, the SDKs supply the auditor's `u` |

No constraint ties `V_i` to the intended recipient. A record sealed to another key credits nobody, since a reader accepts a note only when the commitment rebuilds. The sender chooses `V_i`.

## Slot body

A non-exit slot's body is one `CRING/rcp` stream under `CRDS_i`. The payload continues the record's stream at the record's published length.

```text
body    = record_ct || payload_ct                        payload_ct only under the frame's HAS_DATA
payload = u16be(len) || application_data || zero_padding
```

`len` counts the application data alone, so an empty payload is `u16be(0)`. The record carries the hashes the commitment covers. The decoder refuses nonzero bytes after the declared data. No proof reads the payload.

Under `HAS_DATA` every non-exit body in a frame has one width, and the program refuses unequal widths. Without it every non-exit body is the published record alone.

SPP output data of a non-exit slot is scheme 10, `u16le(body_len) || body || reply`. SPP's external data hash binds its bytes. An exit keeps SPP's scheme 3 body, and its record travels in the auditor message. A recipient accepts a note only when the record names its own owner key as a ring output and the commitment rebuilds from the record and the frame's tree id, with zero for a hash the flags leave out. A reader without the frame tries each form the body's length admits. The note spends only once its nullifier key is the zero key or the owner's enrolled one.

## Reply metadata

A frame opts in with `REPLY_METADATA`, on an anonymous frame only. The reply follows its body inside the slot's scheme 10 data, under `CRING/rpl` and the same slot secret:

```text
viewing_u || owner_key || nullifier_pk || wincode(Data), zero padded to reply_len
```

`reply_len` is one length per frame, at least 97. Every non-exit slot carries one, and a slot with no reply or a dummy carries random bytes of that length. SPP's external data hash binds the bytes, and no proof reads their meaning. A reply is an unauthenticated claim about the sender. A reader never turns it into a verified sender identity.

## Opening a transfer

1. The auditor computes `X25519(auditor_sk, tx_viewing_u)`, the salted [stream secret](envelope.md#shared-secret) and the stream. It recovers `tx_viewing_sk`, `R` and any entry records from the envelope, `V_i` from each key region, and on a vouched frame the voucher's key from the voucher region.
2. It derives `CRDS_i` from `tx_viewing_sk` and `V_i`, and decrypts slot `i`'s record from the slot's body. An exit's record comes from the auditor message.
3. It rebuilds slot `i`'s commitment from the record and the frame's tree id, with zero for a hash the flags leave out. The ring id enters on kind `1`, and zero on kind `2`. It accepts the slot only when that commitment equals the published hash at `i`. A kind `0` slot rebuilds as SPP's dummy hash under `TXOB(n0, TXOS(n0, CRBS(tx_viewing_sk, salt)), i)`. A record moved to another slot rebuilds nothing.
4. It links the nullifier of each spent note, see [spent notes](#spent-notes).

## Layout

| Bit value | Name | Set when |
|---|---|---|
| 1 | `vouched` | the frame publishes the voucher region |
| 2 | `entry` | the frame selects a key with entry capacity |
| 4 | `data` | the frame sets `HAS_DATA`, every record publishes `data_hash` |
| 8 | `ring_data` | the frame sets `RING_DATA`, every non-exit record publishes `ring_data_hash` |

The layout value is `vouched | entry << 1 | data << 2 | ring_data << 3`. The circuit decomposes it, binds `entry` to the key and refuses every bit past the four. `vouched` switches the voucher's record read and its region in the stream. `data` and `ring_data` fix each record's published prefix.

## Merges

A merge seals one record of its output, followed by the seed, to the merged owner's viewing key `V` from a fresh `eph_sk`. An envelope seals `eph_sk` and `V` to the auditor.

```text
merge_envelope  = eph_sk || V                                    64 bytes, CRING/mrg under the shared secret to the auditor
merge_record    = head || fe32(nullifier_pk) || fe32(seed)       169 bytes, kind 1, blinding TMOB(seed, n0)
merge_record_ct = CRING/mro stream under the slot secret of eph_sk and V, salt zero, index 0, the merge's n0
```

The seed is the owner's nullifier secret. The ring proof also binds SPP's merge output ciphertext to that seed, the amount and the mint, since SPP proves none on a ring merge. The owner opens the record with its viewing secret, and the auditor opens the envelope first. Both ciphertexts travel in the ring instruction, since SPP's merge publishes no auditor message. A merge's ring proof opens no input, since SPP's merge proof ties every input to the output's owner, asset and sum. [Envelope uses](envelope.md#envelope-uses) states the keys and counters.
