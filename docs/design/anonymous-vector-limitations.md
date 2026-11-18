# Anonymous vector limitations

Zolana's `test-vectors/anonymous_eddsa_accepting.json` lists transactions that the SPP anonymous EdDSA circuit (`transfer-eddsa-ring`) accepts, one functional path each. `TestAnonymousEddsaVectorsSolveTheHiddenRail` in `prover/circuits/transfer/anonymous_vectors_test.go` replays every vector on the hidden rail of a transfer key. A vector that the transfer key refuses marks a restriction that the ring adds on top of SPP.

The test skips each vector named in `anonymousVectorLimitations`, and this file explains why. The vectors stay in the file so the limitations remain visible. `RUN_ANONYMOUS_VECTOR_LIMITATIONS=1` runs them anyway, and each one fails at the frame named below. Once a restriction is lifted, its vector solves and its entry here comes out.

## How a vector becomes a ring witness

- The test picks the smallest catalog key whose inputs hold the notes and published dummies and whose outputs hold every published output. Address nullifiers enter the separate address chain in their original order. When a default-ring note is spent, it picks an entry key. Output slots on the key beyond the vector's published outputs are compact padding, since SPP never publishes them.
- The ring is plain, with no policy hash and no own root. A policy restricts on purpose, so these vectors test the composition and leave out the rules.
- Every slot keeps its kind, owner identity and key, nullifier key, mint, asset, amount, data hashes and tree id. An input keeps its blinding.
- A vector's ring slot names the fixture ring's program id. Output blindings come from the ring's sealed key, which is the derivation that the ring holds SPP's blinding seed to.
- The published signer vector's non-zero entries form the run, in order.
- Every negative public amount becomes one withdrawal leg of the same asset and amount, paid to a fixed owner. The program reads legs from the settlement accounts, and SPP does not publish them.
- The ring opens no tree slot roots, cache selection or Merkle paths. Vectors for those paths pass as long as the slots keep the tree ids that the vector names.

## Limitations

### The first note must belong to the hidden owner

The hidden rail reads the hidden owner from its first note and holds that note outside the run (`transfer.(*TransferCircuit).hiddenOwner`). SPP lets the hidden owner's note sit after a public signer's note. Addresses are separate from the ring's note inputs, so an address before a hidden-owner note is accepted.

| Vector | Refusal |
| --- | --- |
| `signer_then_hidden` | The first note belongs to a public signer, making that signer the hidden owner, and the signer is in the run. |
| `hidden_address_only_with_deposit` | A hidden owner that only creates an address spends no note at all. |

When a transaction has at least one hidden-owner note, the wallet can reorder its inputs, because SPP accepts any order of notes and addresses ahead of the dummies. A hidden-owner address with no hidden note has no hidden-rail proof at all. Creating that address anonymously needs a different rail, or a hidden note spent beside it.

### No hidden-rail key has more than 8 outputs

Every transfer key in `transfer.Shapes` has at most 8 outputs. The single-asset payout key has been removed, so SPP's `1x16` and `2x16` anonymous shapes have no ring counterpart.

| Vector | Refusal |
| --- | --- |
| `one_input_sixteen_outputs` | No catalog key fits the vector's 16 published outputs. |

A wallet splits the payment across transfers of at most 8 outputs.
