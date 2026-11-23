# Storage

Storage keeps tree roots for rings and curators, and verifies the proof behind every root it accepts. Start with [the instruction entry point](src/lib.rs). The layouts and `StorageError` live in `custom_ring_interface::{storage, tracking, owner_grant}`, and the instruction data in `custom_ring_interface::storage_instruction`.

## Policy registries

A creator makes a registry at the PDA `[b"storage", creator, registry_id]`. The creator and the payer both sign, and the creator becomes the first authority. Initialization fixes the grace period. The initial root holds only the tree sentinel.

An update carries record writes and an apply proof. The program checks the authority signature, turns each write into its tree entry, and chains the entries in order from a start that binds the registry address and the expected revision. The proof binds the old root and leaf index to their replacements.

The apply proof classifies an update, and the statement binds its [restrictive](../../docs/design/records-and-roots.md#root-history) bit.

A restrictive batch takes effect with zero grace. It clears every earlier root in the same instruction, even at a full history, so an older root never outlives it. Proofs in flight at an earlier root prove again.

Any other batch, an allowlist addition or a Reader grant, gives the outgoing root a fixed expiry slot, saturating at `u64::MAX`. Over a full history it evicts the oldest kept root. Every kept root reads each policy field no more permissively than the newest root. A proof under a kept root therefore admits no transfer the newest root refuses. `StorageRegistry::root` is the one rule for which root a revision names at a slot.

The authority hands the registry to a new authority with `UpdateAuthority`. Both keys sign, so a typo cannot lock the registry. The old authority loses every right at once.

## Private writes

A registry created with a nonzero `tracking_registry`, a ring's own or a curator's, may back a salted list. Its private entries are ordinary record writes under salted member keys. The ring refuses a frame that reads a salted list through a source pinned to another tracking registry, because this program does not know which ring a source serves.

The program classes each write from its masks, and no caller flag selects the class. A write that touches no private list needs no tracking row. A private write that names an asset entry passes when the mint hashes to the written member. Every other private write sets its bit in `private_writes`, and the apply proof shows the owner's row and salted key under the pinned registry's root.

A private update names a tracking revision and passes the pinned tracking registry after the registry. The program reads that registry's root at the revision, served under grace, and the statement binds it. After the proof it raises `tracking_floor` to the revision together with the new root, and a refusal writes neither. A ring accepts a tracking root for a salted read only at or above the floor. Rows never leave the tracking tree, so an owner with a private entry never proves itself unenrolled.

A public update names no tracking revision and passes no tracking registry, and a registry with a zero `tracking_registry` refuses every private write.

## Tracking registries

A ring creates one tracking registry at `[b"tracking_registry", creator, ring_id]`, pinning its ring id and auditor key. Neither the seeds nor the ring id authenticate it. Anyone can create one naming any ring, and only the address a ring pins in its policy counts.

The ring's enrollment writer signs `EnrollBatch`. Wallets can give the writer an Ed25519 signature for each write. For a PDA, prepare one write with `Enroll::prove` and `OwnerEnrollment::prove_cpi`. Before calling `invoke_signed`, the PDA's program must verify the ring program, the tracking registry, and the enrollment write against its own policy.

Storage checks owner consent, verifies the tree update, and publishes the new root. A first enrollment also requires a `RegisterKey` proof for the sealed nullifier key and list salt. A rotation changes only the viewing key. Rebuild the apply proof if another write advances the registry revision. If storage returns `RootHistoryRetained`, wait for the oldest root to expire before retrying.

### Enroll a vault PDA

Enable the `enrollment-cpi` feature of `custom-ring-interface` in the vault program. Pass instruction data that starts with `ENROLL_PDA` to `PdaEnrollment::parse` unchanged.

Check that the vault program owns `enrollment.vault()` before reading its manager, ring and tracking registry. Derive the expected vault address from your program's seeds. If the vault restricts enrollment keys, check `enrollment.change()` before signing.

```rust
use custom_ring_interface::pda_enrollment::{PdaEnrollment, VaultEnrollmentPolicy};

let enrollment = PdaEnrollment::parse(accounts, data)?;
enrollment.invoke_signed(
    VaultEnrollmentPolicy {
        manager: &vault.manager,
        vault: &vault_address,
        ring: &vault.ring,
        tracking_registry: &vault.tracking_registry,
    },
    signer,
)
```

Pass the vault's seeds and bump in the Pinocchio `Signer`. The helper requires the vault manager's signature before it invokes the ring. The [example vault program](../../examples/enrollment-vault/src/lib.rs) keeps one vault at one PDA.

Set `Enroll::owner` to the vault PDA.

```rust
use custom_ring_sdk::EnrollVault;

let signature = EnrollVault {
    enrollment,
    program: vault_program,
    manager: &manager,
}
.send(&ring_rpc, &client)
.await?;
```

The manager signs the transaction after the SDK prepares the proof. Ring RPC adds the enrollment writer's signature and pays the fee. `send` returns the transaction signature after confirmation.

## Owner grants

`OwnerGrant` lets a program owner name a reader through CPI, since a PDA cannot sign an off-chain read. A wallet signs its Reader grant, and the record writer takes that grant only on the wallet's signature. The owner alone writes the grant account and can revoke it. Ring RPC reads the account at the grant's address, which names the tracking registry, owner and reader.

## Limits

An enrollment is public, anyone can see a wallet enrolled on a ring. Accounts created here are never closed, which is what prices a flood of first enrollments. The program verifies no policy. A consumer reads a root and authenticates the registry, the rules and the action before it acts.

## Tests

Build and run the lifecycle test from the workspace root.

```sh
just build-storage-program
cargo test --locked -p storage-program --test lifecycle
```

Tests that settle a real proof read [proofs.json](tests/proofs.json), a key rotation regenerates it. The others seed accounts and stop at the proof check, so they show the checks ahead of the pairing and never that a proof settles. To replace the fixtures, install the pinned ring proving keys and run the generator from `prover/`. The proofs bind the registry address, and the test refuses a fixture made for another address.

```sh
STORAGE_REGISTRY="$(jq -r .registry ../programs/storage/tests/proofs.json)" \
REQUIRE_RING_KEYS=1 go test ./ring -run '^TestStorageFixtures$' -count=1
```

`ZOLANA_PROVER_KEYS_DIR` points the generator at another key directory.
