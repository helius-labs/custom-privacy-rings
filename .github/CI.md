# CI

The `ci` workflow runs on pushes to `main`, pull request changes and manual dispatch.
Draft pull requests run the Zolana pin check and skip the test jobs.

Manual dispatch defaults to `validation=full` and runs the complete localnet matrix and TypeScript live tests.
Only a manual dispatch runs the widest frame and full capacity sweeps, a pull request and a push to `main` leave those eight jobs out.
Set `validation=sanity` to run `just test-custom-ring-sanity` as the localnet suite.
That mode omits TypeScript live tests and keeps the build, lint, unit, package and documentation checks.
It reports `ci sanity result`, which does not replace the required `ci result` check.

The TypeScript suites seed `crypto.getRandomValues` from `ZOLANA_RANDOM_SEED`, fixed in the workflow env, and a Rust test that seeds its draws reads it through `spr_test_utils::seed::random_seed`.
Dispatch with `random_seed` to run them under another seed.
A failing TypeScript test prints its seed and the replay command, a seeded Rust test prints its seed.

The `keys` job downloads every ring proving key and the SPP ring transfer keys once and saves them under the hash of both lockfiles.
The localnet and e2e jobs restore that entry and download only a key it lacks.
A pull request that changes a lockfile saves its own entry, the jobs of that pull request read it.

The localnet matrix runs each sweep of `program-tests/custom-ring/tests/ring_shapes.rs` as its own job.
The sweeps of a full dispatch prove every catalog key.
The ring localnet ports of `tools/test-ring-controls.sh` stay below 32768, the start of the Linux ephemeral range, so the kernel gives none of them to a client socket.
The lifecycle slices run `program_owner` with `enrollment_vault.so` from the `ring_program` artifact.
The `program update` job runs `just test-custom-ring-program-update`, the two `program_update` tests of a Xark program on a ring, with real proofs.
`build / programs` ships `zk_ring_program.so` in `ring_program`, and `build / zolana cli` compiles the two circuits with their insecure test keys into `zk_ring_circuits` and fails when the verifier module it writes differs from the committed one.
`build / ring tools` builds a second nextest archive of the `ring` binary with the `proofs` feature under `target/nextest-archives/proofs`, and only the `program update` job reads it.
The other localnet jobs run the archive without `proofs`, so `one_key` runs only from `just one-key-validation`.
The localnet and TypeScript live jobs start `tools/localnet-kms.mjs`, a local KMS emulator, on the pinned Node.
A failed localnet job prints the end of the Photon and ring indexer logs, and a failed TypeScript live job prints the end of each service log under `target/ts-sdk-e2e`.
CI omits prover logs from failure reports because proof errors can contain private witness values.

Dispatch the final pushed branch after the Zolana pin passes its release source check and the pinned proving keys are public.
The `prover` and `ring-indexer` workflows have separate manual dispatches.
The prover workflow requires the pinned keys for its installed circuit and anonymous tag checks.
Its path filter names `sdk-libs/client/**`, the crate whose `tests/fixtures` hold the Rust request goldens the Go prover tests read.
The sanity mode does not run those workflows.
