These are the target requirements of PR #17, each with its implementation status. [circuit spec.md](circuit%20spec.md) explains the mechanisms and the status words. Status refers to the merged ring-core head `823e355f`. Transfer and merge circuits have no wired verifying key there, and nothing is validated with real proofs on release keys.

**Use cases:**

1. KYC-gated payments. Status: Implemented, through allow rules on senders and output owners.
2. Sanctions screening. Status: Implemented, through block rules, public or private.
3. Transaction monitoring. Status: Implemented, through the proven auditor stream and Ring RPC readers.
4. Freezing funds. Status: Implemented, through a sender rule over the Frozen list. A frozen owner can still receive and merge, and the delegate can still move its notes.
5. Recovering funds through a permanent delegate. Status: Partially. Moves within the ring and withdrawals are built. Exits wait for an SPP verifying key.
6. Small payments without KYC, with KYC required above a threshold. Status: Implemented, through an amount guard on an allow rule.

**Features:**

1. **Auditor recovery**

   **Purpose:**
   1. Let the auditor monitor ring transactions. Status: Implemented.
   2. Let Ring RPC serve decrypted balances and history to authorized users. Status: Implemented.

      **Technical requirements:**
   3. Verifiably encrypt each UTXO’s owner, asset, amount and blinding to the auditor. Status: Implemented. Owner, asset and amount are in the proven record. The blinding derives from the sealed secret on transfers, is sealed on deposits, derives from a sealed seed on merges and is opened for entry inputs.
   4. **Linkability:** Let the auditor associate published nullifiers with spent UTXOs while hiding these spend links from public observers. Status: Implemented. The auditor derives nullifiers from each owner's sealed nullifier secret.
   5. **Cases:**
      1. **Transfer:** can be combined with other cases with the same flexibility as SPP circuit, auditor can recover amount, asset, owner, blinding of every real output utxo and link inputs. Status: Implemented.
      2. **Entry from the default ring:** Recover inputs created outside the auditor’s scope. Status: Implemented. Entry keys open each default-ring input in the stream, and one frame can hold several owners.
      3. **Exit to the default ring:** Recover the created default-ring UTXO. Must be in the same layout as standard confidential transfers. Status: Implemented. A kind 2 record opens it, and the recipient body is SPP's confidential layout.
      4. **Withdrawal:** No additional encrypted information required. Status: Implemented.
      5. **Deposit:** Same as transfer. Status: Implemented. One proven opening record per deposit output.
      6. **Merge:** Same as transfer. Status: Partially. The merge circuit seals the output record, but merge has no verifying key and its SPP selector is not enabled.
      7. **Delegate transfer:** Same as transfer. Status: Implemented. The same circuit and stream serve the delegate rail.
      8. **Compact padding:** must be supported same as SPP circuit. Status: Implemented for outputs. Interleaved note and address inputs are in progress.
      9. **Dummies:** Indistinguishable from real outputs through public information. The auditor can reliably identify and skip them. Status: Implemented. A dummy's record is kind 0 and all zero. Its public tag and body are random, and that part rests on the SDK.

   - **Status:** Partially.
   - **Why:** Recovery is proven on every rail, but merge has no verifying key and its SPP selector is refused, and the interleaved input layout is in progress.
   - **How:** Verifiable encryption to the auditor. One AES stream per transfer holds one record per published slot, blindings derive from a sealed secret, and spends link through nullifiers the auditor derives from secrets sealed at enrollment.
   - **Argument:** The auditor rebuilds each published commitment from its record. A record that does not match its slot rebuilds nothing, so a valid proof means a readable output.
   - **Code:** `prover/circuits/audit/audit.go`, `prover/circuits/audit/record.go`, `prover/circuits/merge.go`, `services/ring-rpc/src/operations/utxos.rs`.

2. **Privacy**

   **Purpose:**
   1. Protect users’ financial activity from public observers while allowing auditor access. Status: Implemented.

      **Technical requirements:**
   2. Hide private UTXO amounts, assets and blindings from public observers. Status: Implemented on both ring modes.
   3. **Confidential transfers:** Sender identities, recipient viewing keys and policy-required owner identities may be public. Status: Implemented. The sender is public, and each slot tag is the last 8 bytes of the recipient's viewing key `u`.
   4. **Anonymous transfers:** Hide the anonymous sender’s identity and ring recipient identities. Explicit public co-owners, default-ring input owners and exit owners remain public. Status: Implemented, under the conditions in `docs/design/privacy.md`. For example, the payer must not link to the owner, and the prover must run locally or in an attested enclave.
   5. Public deposit and withdrawal amounts and assets, transaction structure and the grouping of outputs within a transaction may remain visible. Status: Implemented. Enrollment is also public.

   - **Status:** Implemented.
   - **Why:** Amounts, assets and blindings stay in private inputs and ciphertexts on every rail.
   - **How:** The hidden rail signs inside the SPP proof and publishes no owner. Policy checks read private records, so screening names no owner.
   - **Argument:** Every rule, cap and guard runs inside the proof, so only the proof's public inputs reach the chain.
   - **Code:** `docs/design/privacy.md`, `program-libs/interface/src/codec/stream.rs`, `prover/circuits/transfer/owners.go`.

3. **Composability**

   **Purpose:**
   1. Let applications build custom-ring transactions with the same transaction flexibility as SPP. Status: Partially. Rust and TypeScript builders cover every ring operation. No builder for a third-party program that calls the ring transfer through CPI was found.

      **Technical requirements:**
   2. Support program-owned UTXOs and CPI flows. Status: Partially. Program-owned notes use the zero nullifier key, and signer flags pass through the ring program to SPP. A validator test sends a PDA owner's `Enroll` through CPI. Its apply step needs the missing `ApplyEnrollments` key. No test shows a PDA owner spending a ring note through a CPI into the ring transfer.
   3. Preserve SPP’s supported owner combinations and signing methods. Status: Partially. The circuit serves signer, in-proof, hidden, public-owner and delegate rails, and entry keys hold several owners. The TypeScript SDK still refuses co-owners on an entry spend.
   4. Preserve SPP’s supported input/output sizes and compact and dummy padding. Status: Partially. Off entry, the catalog reaches every SPP width, and output padding matches SPP. Entry keys are narrower and have no 16-output key. Interleaved note and address inputs are in progress.

   - **Status:** Partially.
   - **Why:** The circuit and catalog match SPP, but the SDK entry check, the interleaved input layout and a CPI spend test are open.
   - **How:** One transfer circuit with rail bits and a catalog of one Groth16 key per shape. A frame larger than one transaction is submitted from a frame buffer by its SHA-256 digest.
   - **Argument:** A frame takes the first catalog row that holds its SPP width, so every width up to the catalog ceiling has a key.
   - **Code:** `program-libs/interface/src/proof/key_catalog.rs`, `programs/custom-ring/src/instructions/runtime/instruction_runtime.rs`, `programs/custom-ring/src/instructions/settlement/frame_buffer.rs`, `sdk-libs/ts/src/operations/payments/anonymous.ts`.

4. **Owner allowlisting**

   **Purpose:**
   1. Restrict sending or receiving funds to approved owners. Status: Implemented.

      **Technical requirements:**
   2. Allowlist entries may be public or private. Private entries must not reveal members’ identities to public observers. Status: Partially. PrivateAllow stores entries under salted keys. A known floor gap remains. A private write made while its target's enrollment is still pending sets the floor below that enrollment, so the target can later prove it is not enrolled and skip its entry. The credential switch in progress closes it.
   3. Using the allowlist must not reveal the identities of the owners sending or receiving funds to public observers. Status: Implemented. The prover of a transfer still learns the list status of the other owners in that proof.

      **Cases:**

   Apply the configured sender and recipient allowlisting to each case, regardless of which instruction handles it.
   4. **Transfer within the ring:** Check spent UTXO owners and receiving UTXO owners. Status: Implemented.
   5. **Deposit:** Check receiving ring UTXO owners. Status: Implemented.
   6. **Withdrawal:** Check spent UTXO owners and public withdrawal recipients. Status: Implemented.
   7. **Entry from the default ring:** Check spent default-ring UTXO owners and receiving ring UTXO owners. Status: Implemented.
   8. **Exit to the default ring:** Check spent ring UTXO owners and receiving default-ring UTXO owners. Status: Implemented.
   9. **Merge:** No allowlist checks; ownership remains unchanged. Status: Implemented.
   10. **Delegate transfer:** No allowlist checks; authorization is covered by the permanent delegate policy. Status: Implemented.

   - **Status:** Partially.
   - **Why:** Every case is screened, but private entries carry the floor gap until the credential switch lands.
   - **How:** Grant rules read presence on Allow, Approval or PrivateAllow in a sorted policy record tree. Every live owner row and every live payment owner is a subject.
   - **Argument:** The program sets the screening gate from the policy hash and the rail, so no prover input can skip a subject.
   - **Code:** `prover/circuits/policy/subjects.go`, `prover/circuits/policy/rules.go`, `program-libs/policy/src/list.rs`.

5. **Owner blocklisting and freezing**

   **Purpose:**
   1. Prevent selected owners from sending or receiving funds. Status: Implemented.

      **Technical requirements:**
   2. Blocklist and freeze-list entries may be public or private. Private entries must not reveal members’ identities to public observers. Status: Partially. PrivateBlock exists. No private Frozen list exists, so a private freeze shares the PrivateBlock list. Private entries carry the floor gap of 4.2.
   3. Using the blocklist must not reveal the identities of the owners sending or receiving funds to public observers. Status: Implemented.
   4. Blocking and freezing can use separate lists. Status: Implemented for public lists.

      **Cases:**

   Apply the configured sender and recipient restrictions to each case.
   5. **Transfer within the ring:** Check spent UTXO owners and receiving UTXO owners. Status: Implemented.
   6. **Deposit:** Check receiving ring UTXO owners. Status: Implemented.
   7. **Withdrawal:** Check spent UTXO owners and public withdrawal recipients. Status: Implemented.
   8. **Entry from the default ring:** Check spent default-ring UTXO owners and receiving ring UTXO owners. Status: Implemented.
   9. **Exit to the default ring:** Check spent ring UTXO owners and receiving default-ring UTXO owners. Status: Implemented.
   10. **Merge:** No blocklist checks; ownership remains unchanged. Status: Implemented.
   11. **Delegate transfer:** No blocklist checks; authorization is covered by the permanent delegate policy. Status: Implemented.

   - **Status:** Partially.
   - **Why:** All cases are screened, but a private freeze list and the floor fix are missing.
   - **How:** Deny rules read absence from Block, Frozen or PrivateBlock. Absence is proven by opening the predecessor record with `member < subject < next`.
   - **Argument:** A deny rule reads exactly one list from one source tree, so the prover cannot pick a tree that lacks the entry.
   - **Code:** `prover/circuits/policy/rules.go`, `program-libs/policy/src/rule_table/mod.rs`, `program-libs/member-registry/src/record.rs`.

6. **Asset allowlist**

   **Purpose:**
   1. Restrict assets entering, moving within or leaving the ring to approved assets. Status: Implemented.

      **Technical requirements:**
   2. Enforcing the asset allowlist must not reveal private UTXO assets to public observers. Status: Implemented.

      **Cases:**
   3. **Transfer within the ring:** Check assets of spent and receiving UTXOs. Status: Implemented. An unguarded asset rule also checks every live input asset.
   4. **Deposit:** Check deposited assets. Status: Implemented.
   5. **Withdrawal:** Check withdrawn assets. Status: Implemented.
   6. **Entry from the default ring:** Check assets entering the ring. Status: Implemented.
   7. **Exit to the default ring:** Check assets leaving the ring. Status: Implemented.
   8. **Merge:** No asset-allowlist checks; the asset remains unchanged. Status: Implemented.
   9. **Delegate transfer:** No asset-allowlist checks; authorization is covered by the permanent delegate policy. Status: Implemented. A delegate deposit leg is refused, so no new asset enters on that path.

   - **Status:** Implemented.
   - **Why:** Asset rules screen every payment and input asset in the transfer and deposit circuits.
   - **How:** An asset member is `hash_bytes(mint)` in the same record tree, never salted.
   - **Argument:** The mint is a private input held to the note commitment, so screening it reveals no asset.
   - **Code:** `prover/circuits/policy/rules.go`, `prover/circuits/policy/subjects.go`, `program-libs/policy/src/member.rs`.

7. **Amount guards**

   **Purpose:**
   1. Apply recipient or asset restrictions only when an amount exceeds a configured threshold. Status: Implemented.
   2. use case, part of the custom ring users are not kyced a part are kcyed. Status: Implemented.

      **Technical requirements:**
   3. Evaluating the guard must not reveal private amounts or assets to public observers. Status: Implemented.

      **Cases:**

       Sum receipts per owner and asset for recipient rules, or per asset for asset rules, within the transaction.
   4. **Transfer within the ring:** Evaluate guards using transferred amounts. Status: Implemented. Change to the same owner is netted.
   5. **Deposit:** Evaluate guards using deposited amounts. Status: Implemented.
   6. **Withdrawal:** Evaluate guards using withdrawn amounts. Status: Implemented.
   7. **Entry from the default ring:** Evaluate guards using amounts entering the ring. Status: Implemented.
   8. **Exit to the default ring:** Evaluate guards using amounts leaving the ring. Status: Implemented.
   9. **Merge:** No amount-guard checks. Status: Implemented.
   10. **Delegate transfer:** No amount-guard checks; authorization is covered by the permanent delegate policy. Status: Implemented.

   - **Status:** Implemented.
   - **Why:** `AboveAmount` and `AboveAmountByAsset` guards run in the policy section.
   - **How:** A guard holds one threshold for the rule, or reads it per asset from the asset's record. Sender rules take no guard.
   - **Argument:** The circuit sums receipts per owner and asset before it compares, so splitting a payment into several outputs cannot slip under the threshold.
   - **Code:** `prover/circuits/policy/rules.go`, `program-libs/policy/src/quantities.rs`, `program-libs/policy/src/rule_table/mod.rs`.

8. **Per-transfer outflow caps**

   **Purpose:**
   1. Limit each owner’s net outflow per asset within one transaction. Status: Implemented.

      **Technical requirements:**
   2. Checking the cap must not reveal private amounts, assets or owner identities to public observers. Status: Implemented.

      **Cases:**

       Aggregate outflow per owner and asset across the transaction. Subtract funds returned to the same owner in ring UTXOs.
   3. **Transfer within the ring:** Count funds sent to other owners. Status: Implemented.
   4. **Deposit:** No outflow-cap checks; public funds enter the ring. Status: Implemented.
   5. **Withdrawal:** Count withdrawn funds toward the owner’s outflow. Status: Implemented.
   6. **Entry from the default ring:** Count funds sent to other owners; exclude funds received by the same owner within the ring. Status: Implemented.
   7. **Exit to the default ring:** Count exited funds, including exits to the same owner. Status: Implemented.
   8. **Merge:** No outflow-cap checks. Status: Implemented.
   9. **Delegate transfer:** No outflow-cap checks; authorization is covered by the permanent delegate policy. Status: Implemented.

   - **Status:** Implemented.
   - **Why:** The outflow section of the transfer circuit enforces the cap from the asset's own record.
   - **How:** `outflow = debit + charge`. The debit is what an owner spent less what came back to it. The charge shares out what leaves beyond the summed debits.
   - **Argument:** The charge term stops a deposit-funded return to the sender from hiding funds that leave.
   - **Code:** `prover/circuits/policy/outflow.go`, `program-libs/policy/src/quantities.rs`.

9. **Co-signer approval thresholds**

   **Purpose:**
   1. Require additional approval when an owner’s outflow or public withdrawals exceed configured thresholds. Status: Implemented.

      **Technical requirements:**
   2. Checking thresholds must not reveal private amounts, assets or owner identities. Whether approval is required may be public. Status: Implemented. Only `approval_mask` and a blinded totals commitment are public.
   3. Require approval if any configured threshold is exceeded. Status: Implemented.

      **Cases:**

      Calculate owner outflow as defined by the outflow cap. Aggregate gross public withdrawals per asset across the transaction, without subtracting deposits.
   4. **Transfer within the ring:** Apply owner-outflow thresholds to funds sent to other owners. Status: Implemented.
   5. **Deposit:** No outflow or withdrawal threshold applies to a deposit alone. Status: Implemented.
   6. **Withdrawal:** Apply both owner-outflow and gross-withdrawal thresholds. Status: Implemented.
   7. **Entry from the default ring:** Apply owner-outflow thresholds to funds sent to other owners; exclude funds received by the same owner within the ring. Status: Implemented.
   8. **Exit to the default ring:** Apply owner-outflow thresholds, including exits to the same owner. Status: Implemented.
   9. **Merge:** No approval-threshold checks. Status: Implemented.
   10. **Delegate transfer:** No approval-threshold checks; authorization is covered by the permanent delegate policy. Status: Implemented.

   - **Status:** Implemented.
   - **Why:** The proof sets the approval bits, and the program demands an approver for each set bit.
   - **How:** `APPROVE_OUTFLOW` is set when an owner's outflow passes the asset's co-sign bound. `APPROVE_WITHDRAWAL` is set when the gross withdrawal passes the asset's withdrawal threshold. A set bit demands its class even when the ring configured no co-signer.
   - **Argument:** The circuit asserts the mask equal to the computed bits, and a set bit fails closed with no approver, so a crossed threshold fails without its approver.
   - **Code:** `prover/circuits/policy/outflow.go`, `program-libs/interface/src/instructions/cosign.rs`, `programs/custom-ring/src/instructions/transfer/accounts.rs`.

10. **Permanent delegate**

    **Purpose:**

    1. Allow a configured delegate to move funds without owner authorization. Status: Implemented.

    **Technical requirements:**

    1. Require authorization from the delegate configured by the ring policy. Status: Implemented. `SetDelegate` sets it once with the admin and the delegate signing. SPP governance must also enable the ring's delegate rail.
    2. Delegate transfers bypass the other transfer policies; auditor recovery remains required. Status: Implemented.

    **Cases:**

    1. **Transfer within the ring:** The delegate may move funds between owners without their signatures. Status: Implemented.
    2. **Deposit:** Delegate authority cannot authorize spending public funds. Status: Implemented. A deposit leg is refused.
    3. **Withdrawal:** Permitted with delegate authorization. Status: Partially. The program accepts it. The Ring RPC request and the CLI carry no withdrawal, and the Rust and TypeScript builders do.
    4. **Entry from the default ring:** Delegate authority cannot spend default-ring UTXOs. Status: Implemented. The program refuses entry on the delegate rail and no key exists for it.
    5. **Exit to the default ring:** Permitted with delegate authorization. Status: Partially. The rail selects SPP's `RingAuthorityExit`, which has no SPP verifying key yet.
    6. **Merge:** Normal merge authorization applies; no additional delegate-policy checks. Status: Implemented.
    7. **Delegate transfer:** Require the configured delegate’s authorization and apply auditor recovery, without the other transfer-policy checks. Status: Implemented.

    - **Status:** Partially.
    - **Why:** The bypass rail is built, but exits cannot verify on SPP yet, and the delegate's signing side does not yet check which notes a Ring RPC message spends.
    - **How:** The instruction tag sets the delegate bit. The circuit then holds every policy root to zero and keeps the enrollment checks and the audit stream. Ring RPC opens the source's nullifier secret with the auditor key and checks each note's nullifier against it.
    - **Argument:** The bypass comes from the instruction tag, not from frame data, so only the delegate's own instruction can skip policy.
    - **Code:** `programs/custom-ring/src/instructions/transfer/accounts.rs`, `prover/circuits/transfer/transfer.go`, `sdk-libs/sdk/src/operations/delegate_source.rs`, `services/ring-rpc/src/operations/authority_transfer.rs`.

&nbsp;

**Notes:**

1. With input and output allowlisting, removal blocks sending and receiving; a separate freeze list only adds “receive but not spend.” Status: Implemented. A sender rule over Frozen gives receive but not spend.
2. Should we add asset block list? Status: Implemented as a rule form. An asset rule over Block refuses listed assets. No asset-only deny list exists.
3. Should amount guards also add a min transfer? Status: Deferred. Guards compare above a threshold only.
4. Do we need Co-signer approval thresholds. at all or is a co signer policy in the relayer sufficient? Status: Implemented as both. The transfer circuit computes the approval bits, and the program demands a co-signer for each set bit. Ring RPC enforces window limits when it co-signs.
