# CI

The `ci` workflow runs on pushes to `main`, pull request changes and manual dispatch.
Draft pull requests run the Zolana pin check and skip the test jobs.

Manual dispatch defaults to `validation=full` and runs the complete localnet matrix and TypeScript live tests.
Only a manual dispatch runs the widest frame and full capacity sweeps, a pull request and a push to `main` leave those eight jobs out.
Set `validation=sanity` to run `just test-custom-ring-sanity` as the localnet suite.
That mode omits TypeScript live tests and keeps the build, lint, unit, package and documentation checks.
It reports `ci sanity result`, which does not replace the required `ci result` check.
Set `validation=sweeps` to run those eight sweeps alone as the localnet suite, `gh workflow run ci --ref <branch> -f validation=sweeps`.
Add `-f sweeps="widest frames"` or `-f sweeps="full capacity"` to run one group of four.
That mode omits TypeScript live tests too and reports `ci sweeps result`.

The TypeScript suites seed `crypto.getRandomValues` from `ZOLANA_RANDOM_SEED`, fixed in the workflow env, and a Rust test that seeds its draws reads it through `spr_test_utils::seed::random_seed`.
Dispatch with `random_seed` to run them under another seed.
A failing TypeScript test prints its seed and the replay command, a seeded Rust test prints its seed.

The proving key cache entry is named by a hash of `just custom-ring-key-pins`, the name and digest of each ring key and of the two SPP ring transfer keys.
Only a run on `main` saves that entry.
Elsewhere the `keys` job of a run that finds no entry downloads the keys once and uploads them as the run artifact `proving_keys`.
Each job that reads the keys restores the entry, or else that artifact, then `fetch-custom-ring-keys` holds every key to its lockfile digest and downloads a key only when it is missing or off its digest.
A pull request saves no entry, each one is a full copy of the keys under the repository cache limit.
The `setup` action saves its Rust caches on `main` alone.

The Squads program and surfpool reach the test jobs inside the `zolana_programs` and `zolana_tools` artifacts.
`build / programs` dumps the Squads program from mainnet when its cache entry is absent, `build / zolana cli` downloads the surfpool release the same way.
`just ensure-smart-account` and `just ensure-surfpool` hold both to the digests the `justfile` pins, in their build job and again in the validator test recipes.
A Squads upgrade on mainnet fails `ensure-smart-account` until the pin moves.

The localnet matrix runs the four large sweeps of `program-tests/custom-ring/tests/ring_shapes.rs` in parts.
`RING_SWEEP_PART=<k>/<n>` runs the shape loop of a sweep over each n-th shape and `RING_SWEEP_PART=keys` runs the phases after the loop, an unset variable runs the whole sweep.
The `changes` job builds those jobs from its `COLUMNS` table, and a test in `ring_shapes.rs` checks that the parts of a sweep hold each shape once.
The lifecycle slices leave out the `shapes` key and withdrawal tests, the sweep jobs of the same run prove them.
The sweeps of a full dispatch prove every catalog key.
The ring localnet ports of `tools/test-ring-controls.sh` stay below 32768, the start of the Linux ephemeral range, so the kernel gives none of them to a client socket.
The lifecycle slices run `program_owner` with `enrollment_vault.so` from the `ring_program` artifact.
The `program update` job runs `just test-custom-ring-program-update`, the two `program_update` tests of a Xark program on a ring, with real proofs.
`build / programs` ships `zk_ring_program.so` in `ring_program`, and `build / zolana cli` compiles the two circuits with their insecure test keys into `zk_ring_circuits` and fails when the verifier module it writes differs from the committed one.
`build / proofs archive` builds the nextest archive of the `ring` binary with the `proofs` feature under `target/nextest-archives/proofs`, and only the `program update` job reads it.
The other localnet jobs run the archive without `proofs`, so `one_key` runs only from `just one-key-validation`.
The localnet and TypeScript live jobs start `tools/localnet-kms.mjs`, a local KMS emulator, on the pinned Node.
The exit trap of `tools/test-ring-controls.sh` prints the end of each service log of a failed localnet run, and a failed TypeScript live job prints the end of each service log under `target/ts-sdk-e2e`.
CI omits prover logs from failure reports because proof errors can contain private witness values.

Dispatch the final pushed branch after the Zolana pin passes its release source check and the pinned proving keys are public.
The `ring rpc postgres` job runs `just test-ring-rpc-postgres`, the store tests of Ring RPC and its service crates, on a `postgres:16` service.
The recipe fails when `RING_RPC_TEST_POSTGRES_URL` is unset.
The `prover` and `ring-indexer` workflows have separate manual dispatches.
The `ring-indexer` workflow runs the indexer tests on SQLite and on a `postgres:16` service.
Its path filter names `services/**`, `program-libs/**` and `sdk-libs/**` without `sdk-libs/ts/**`, the trees of the crates the indexer links.
The prover workflow runs `go test -p 1 -count=1` on three runners, `circuits/transfer`, `circuits/policy` and every other package of `prover/`.
The Go test cache does not see the vectors and goldens the tests read outside the module, `-count=1` runs them every time.
Its `committed verifiers` job runs `just check-custom-ring-keys` and its `installed keys` job proves with the pinned keys.
Its path filter names `test-vectors/**` and `sdk-libs/client/**`, the files the Go tests read, and the crates whose tests write those vectors.
The sanity mode does not run those workflows.
