# Contracts

This folder contains smart contracts used by the Disciplr backend.

## accountability_vault

The `accountability_vault` contract caps the number of milestones accepted by `create_vault`.

- `MAX_MILESTONES = 32`
- `create_vault` rejects vault creation when `milestones.len() > MAX_MILESTONES`
- This bound protects per-call CPU/storage budgets for loops like `claim`, `slash_on_miss`, and `withdraw`
# Disciplr Smart Contracts

This directory contains Soroban smart contracts for the Disciplr platform.

## Accountability Vault

The `accountability_vault` contract implements time-locked capital vaults on Stellar with milestone-based release conditions.

### Overview

The accountability vault allows users to:
- Host multiple independent vaults on a single contract deployment, keyed by a unique `vault_id`
- Lock funds in a vault with a total amount
- Define milestones with individual amounts that must sum to the total
- Specify a set of verifiers (M-of-N threshold) authorized to validate milestone completion
- Set a guardian address that can pause/unpause the vault in emergencies
- Set success and failure destinations for fund release
- Allow reclaiming residual (dust) token balances to the creator after settlement

### Security Invariants

#### Checks-Effects-Interactions (CEI) Pattern

`slash_on_miss`, `claim`, and `withdraw` (active-vault path) all update and persist vault
state — setting `status` to the terminal value and zeroing `staked` — **before** executing
the external `token::Client::transfer` call. This ensures the vault reaches a terminal state
even if the downstream token call panics or re-enters the contract.

```rust
// CEI: capture transfer values, update and persist state, then call external token.
let slashed = vault.staked;
let failure_destination = vault.failure_destination.clone();
vault.status = VaultStatus::Failed;
vault.staked = 0;
env.storage().instance().set(&DataKey::Vault, &vault);   // ← state committed

token::Client::new(&env, &token_addr).transfer(          // ← external call last
    &env.current_contract_address(),
    &failure_destination,
    &slashed,
);
```

#### Withdraw Refund Semantics

`withdraw` has two behaviours depending on vault state:

| Vault state | Effect |
|-------------|--------|
| `Draft` | Vault transitions to `Cancelled`. No tokens are held, so no transfer occurs. |
| `Active` (no verified check-ins) | Full `staked` amount is returned to the `creator`. Vault transitions to `Cancelled`. |

For the `Active` path the contract guarantees:

- **Creator balance restored** — the token balance of `creator` after `withdraw` equals the
  balance before `stake` (i.e. the original minted/held amount is returned in full).
- **Contract balance zeroed** — the contract holds no tokens after the refund; `vault.staked`
  is set to `0` before the transfer (CEI) and the on-chain token balance drops to `0`.
- **Blocked when paused** — if the guardian has called `emergency_pause`, `withdraw` on an
  `Active` vault returns `Error::Paused`. Draft-vault cancellation is not affected.
- **Blocked after any check-in** — if at least one milestone has been verified,
  `withdraw` returns `Error::Unauthorized` to prevent unilateral fund recovery once
  progress has been committed.

These invariants are tested in `test_withdraw_active_refunds_creator` (balance round-trip)
and `test_withdraw_active_paused_blocked` (pause guard).

#### Emergency Pause (Guardian Role)

A `guardian` address is set at `create_vault` time. The guardian may call:

- `emergency_pause(guardian)` — blocks `slash_on_miss`, `claim`, and active-vault
  `withdraw` while a dispute or incident is investigated.
- `emergency_unpause(guardian)` — re-enables normal operations.

Only the address stored as `vault.guardian` may call these functions; any other address is
rejected with `Error::Unauthorized`. Draft-vault cancellation via `withdraw` is not affected
by the pause flag, as it involves no token transfer.

#### M-of-N Verifier Approvals

`check_in` supports a configurable set of verifiers and an `approval_threshold` (M-of-N).
A milestone is flipped to `verified` only once at least `approval_threshold` distinct
addresses from the verifier set (or the optional oracle) have approved it.

- Double-approval by the same address returns `Error::AlreadyApproved`.
- Approvals are tracked per-milestone in `DataKey::MilestoneApprovals(index)`.
- The threshold must be ≥ 1 and ≤ `verifiers.len()`; otherwise `create_vault` returns
  `Error::InvalidThreshold`.

#### Token Admin Balance Conservation

The token admin account is only used in tests to mint initial balances to the creator and
must not be debited or credited by vault lifecycle operations. Invariant tests assert that
the token admin balance is unchanged across:

- success path: `stake -> check_in -> claim`
- slash path: `stake -> slash_on_miss`

This guards against unintended mint/burn side effects during settlement flows.

#### Terminal-State Rule: Cancelled Vaults Cannot Be Re-Staked

Once a vault transitions to `Cancelled`, staking must remain permanently blocked. Regression
tests lock this down for both cancellation entry points:

- `cancel_vault` path (Draft cancellation)
- `withdraw` path (Active refund cancellation)

In both scenarios, a subsequent `stake` call must fail with `Error::NotDraft`. This preserves
the lifecycle state machine and prevents reopening a terminal vault state.

#### Evidence Hash Binding

`check_in` accepts an `evidence_hash: BytesN<32>` parameter — a 32-byte digest (e.g.
SHA-256) of the off-chain evidence artifact (document, IPFS CID, etc.). When the
approval threshold is reached, the hash is persisted alongside the check-in timestamp
under `DataKey::CheckIn(index)` as a `(u64, BytesN<32>)` tuple and emitted in the
`milestone_checked_in` event value so that the on-chain record is cryptographically bound
to the off-chain evidence.

```rust
// event topics: ("milestone_checked_in", caller, source)
// event value:  (milestone_index, evidence_hash)
env.events().publish(
    (String::from_str(&env, "milestone_checked_in"), caller, source),
    (milestone_index, evidence_hash),
);
```

The backend `submitCheckIn(vaultId, milestoneId, evidenceHash)` passes the hex-encoded
hash, which is decoded to `BytesN<32>` before calling the contract.

### Arithmetic Safety

The `create_vault` function validates that every individual milestone amount is strictly positive (`amount > 0`). If any milestone has an amount `<= 0`, the contract immediately rejects the transaction with `Error::InvalidAmount`, even if valid positive milestone amounts precede or follow it. The sum of all milestone amounts must also match the declared vault `amount` exactly, otherwise the transaction is rejected with `Error::AmountMismatch`.

### Checked Milestone Access

Contract code must not use `unwrap()` when reading milestones by caller-supplied indexes.
Even when a nearby bounds check exists, use checked access such as
`vault.milestones.get(index).ok_or(Error::MilestoneIndexOutOfRange)?` so future
refactors continue to return typed contract errors instead of risking host-level panics.

### Error Types

| Code | Name | Meaning |
|------|------|---------|
| 1 | `AlreadyInitialized` | Vault storage already set |
| 2 | `NotInitialized` | Vault not yet created |
| 3 | `InvalidAmount` | Zero or negative amount |
| 4 | `InvalidDeadline` | Deadline in the past or milestone exceeds vault end |
| 5 | `NoMilestones` | Empty milestone list |
| 6 | `NotDraft` | Expected Draft state |
| 7 | `NotActive` | Expected Active state |
| 8 | `Unauthorized` | Caller not permitted |
| 9 | `AlreadyStaked` | Vault already funded |
| 10 | `MilestoneIndexOutOfRange` | Index beyond milestone list |
| 11 | `MilestoneAlreadyVerified` | Milestone already at threshold |
| 12 | `DeadlinePassed` | Operation rejected after deadline |
| 13 | `DeadlineNotReached` | Slash attempted before deadline |
| 14 | `MilestonesIncomplete` | Not all milestones verified |
| 15 | `NothingToWithdraw` | Staked balance is zero |
| 16 | `AmountMismatch` | Received amount less than declared |
| 17 | `InsufficientAllowance` | Spender allowance below vault amount |
| 18 | `Paused` | Operation blocked by guardian pause |
| 19 | `AlreadyApproved` | Address has already approved this milestone |
| 20 | `NoVerifiers` | Empty verifier list |
| 21 | `InvalidThreshold` | Threshold is 0 or exceeds verifier count |
| 22 | `StakedRemaining` | Reclaim attempted while stake is non-zero |
| 23 | `VaultDisputed` | Operation rejected because vault is in `Disputed` state |

### Performance & Gas Benchmarks

To ensure predictable scaling and prevent out-of-gas exploits or transaction failures, the
contract has built-in performance bounds.

#### Storage Reads & Complexity Analysis

- **Milestone Iteration**: Functions like `claim` and `slash_on_miss` iterate over the
  `milestones` vector. CPU and Memory usage scale linearly (O(N)) with the milestone count N.
- **Flat Storage Access**: The storage layout guarantees flat (O(1)) read footprint. There
  are no redundant storage reads or nested lookups within loops.
- **Gas Bounded Growth**: CPU and Memory bounds are actively asserted in test suites to
  catch regressions before deployment.

#### Documented Footprint Thresholds (10 Milestones Baseline)

| Function | CPU Cost Threshold (Instructions) | Memory Cost Threshold (Bytes) |
|----------|----------------------------------|-------------------------------|
| `create_vault` | < 600,000 | < 200,000 |
| `stake` | < 700,000 | < 200,000 |
| `check_in` | < 300,000 | < 100,000 |
| `claim` | < 900,000 | < 250,000 |
| `slash_on_miss` | < 900,000 | < 250,000 |

### Events

The contract emits `soroban_sdk::Symbol`-typed topics on every state transition.
Symbols are cheaper than `String` on Soroban (lower CPU/memory cost) and are the
idiomatic choice for event keys.

Short topics (≤ 9 characters) use `symbol_short!`; longer topics use
`Symbol::new`.  Both are decoded to plain UTF-8 strings by `scValToNative` in the
Stellar SDK, so off-chain consumers see ordinary strings.

| Symbol topic (on-chain)    | Emitted by                  | Decoded string value       |
|----------------------------|-----------------------------|----------------------------|
| `Symbol::new("vault_created")`    | `create_vault`       | `"vault_created"`          |
| `Symbol::new("vault_staked")`     | `stake`, `stake_from`| `"vault_staked"`           |
| `Symbol::new("milestone_checked_in")` | `check_in`       | `"milestone_checked_in"`   |
| `symbol_short!("oracle")`         | `check_in` (source)  | `"oracle"`                 |
| `symbol_short!("verifier")`       | `check_in` (source)  | `"verifier"`               |
| `Symbol::new("deadline_extended")` | `extend_deadline`   | `"deadline_extended"`      |
| `Symbol::new("vault_slashed")`    | `slash_on_miss`      | `"vault_slashed"`          |
| `Symbol::new("vault_completed")`  | `claim`, `claim_milestone` | `"vault_completed"`  |
| `Symbol::new("vault_cancelled")`  | `withdraw` (Draft)   | `"vault_cancelled"`        |
| `Symbol::new("vault_withdrawn")`  | `withdraw` (Active)  | `"vault_withdrawn"`        |
| `Symbol::new("vault_paused")`     | `emergency_pause`    | `"vault_paused"`           |
| `Symbol::new("vault_unpaused")`   | `emergency_unpause`  | `"vault_unpaused"`         |
| `Symbol::new("milestone_claimed")` | `claim_milestone`   | `"milestone_claimed"`      |

The `eventParser.ts` service maps contract Symbol topic strings to the canonical
`EventType` used by the backend:

```
vault_slashed   → vault_failed    (slash = failure destination settled)
vault_withdrawn → vault_cancelled (active withdraw = cancelled state)
```

Informational topics (`vault_staked`, `milestone_checked_in`, `deadline_extended`,
`vault_paused`, `vault_unpaused`, `milestone_claimed`) are acknowledged by the
parser and silently skipped rather than treated as parse errors.

### Building and Testing

#### Prerequisites

- Rust 1.70+ with `wasm32-unknown-unknown` target
- Soroban CLI tools

#### Build

```bash
cd contracts/accountability_vault
cargo build --release --target wasm32-unknown-unknown
```

#### Test

```bash
cd contracts/accountability_vault
cargo test
```

#### ABI Snapshot

- Snapshot file: `contracts/accountability_vault/spec/AccountabilityVault.spec.json` — checked into the repo.
- To regenerate the snapshot after intentional ABI changes, run the test which will overwrite the file when `UPDATE_SOROBAN_SPEC=1` is set:

```bash
cd contracts/accountability_vault
UPDATE_SOROBAN_SPEC=1 cargo test test_abi_spec_snapshot -- --nocapture
```

Commit the updated JSON together with the contract changes so backend bindings can be reviewed.

### Migration: API change (cancel_vault vs withdraw)

- The contract API now exposes `cancel_vault(vault_id, creator)` for explicitly
  cancelling an unfunded `Draft` vault. This path emits the `vault_cancelled`
  event and performs no token transfers.
- The `withdraw(vault_id, creator)` function has been restricted to the funded
  `Active` refund case (vaults that were staked but never had any verified
  check-ins). It performs a CEI-safe refund to the `creator` and emits
  `vault_withdrawn`.
- Backend callers must choose the appropriate method based on the vault's
  current `status`: use `cancel_vault` for `Draft`, and `withdraw` for
  `Active` refunding. The `vault_cancelled` topic and payload remain
  compatible with the existing backend event parser.


#### Formatting

The workspace ships a `contracts/rustfmt.toml` config. Format all contract sources with:

```bash
cd contracts
cargo fmt
```

#### Lint

The workspace enables `clippy::all` warnings via `[workspace.lints.clippy]` in
`contracts/Cargo.toml`. Run clippy with warnings treated as errors:

```bash
cd contracts
cargo clippy -- -D warnings
```

To suppress known false-positives in generated Soroban SDK code, add
`#[allow(clippy::...)]` at the item level rather than disabling workspace-wide.

#### Test Coverage

The contract maintains comprehensive test coverage including:

- Normal vault lifecycle (create, stake, check-in, claim, slash, withdraw)
- CEI ordering invariants: terminal state committed before token transfer
- Emergency pause/unpause: guardian blocks and re-enables settlement paths
- M-of-N verifier approvals: partial approvals, full threshold, double-approval rejection
- Allowance-based staking (`stake_from`)
- Oracle-driven milestone verification
- Joint deadline extension (`extend_deadline`)
- Disputed state: `admin_dispute` enters hold, `admin_resolve` returns to Active/Completed/Failed, `slash_on_miss` and `claim` blocked while disputed
- Gas benchmarks with hard CPU/memory bounds
- **Claim auth-chain assertions**: `env.auths()` snapshots verifying the recorded authorizer
  matches the claim caller, separately for the creator path and the verifier path

#### Auth-Chain Assertion Pattern

The `claim` function may be called by either the vault creator or any member of the verifier
set. Two dedicated tests in `test.rs` lock down this invariant using `env.auths()` snapshots
rather than a blanket `mock_all_auths`:

**Why `env.auths()`?**
`env.auths()` returns the list of `(Address, AuthorizedInvocation)` pairs that were recorded
during the most recent contract call. Asserting on this list proves that the contract called
`Address::require_auth()` for exactly the address that was passed as `caller`, and not for a
different address. This catches bugs where `require_auth()` is called on the wrong variable
or is missing entirely.

**How the tests work:**

1. Setup (`create_vault`, `stake`, `check_in`) runs under `env.mock_all_auths()` so token
   operations succeed without requiring real signatures.
2. `claim` is invoked with either `creator` or `verifier` as the caller.
3. `env.auths()` is inspected immediately after the call. The test asserts:
   - Exactly one auth entry was recorded.
   - The authorized address equals the claim caller.
   - The authorized function matches `claim(vault_id, caller)` exactly.

```rust
// After contract.claim(&vault_id, &creator):
let recorded = env.auths();
assert_eq!(recorded.len(), 1);
let (addr, invocation) = &recorded[0];
assert_eq!(addr, &creator);
assert_eq!(invocation.function, AuthorizedFunction::Contract((
    contract_id.clone(),
    Symbol::new(&env, "claim"),
    (vault_id.clone(), creator.clone()).into_val(&env),
)));
```

**Helper function:** `assert_claim_auth(env, contract_id, vault_id, caller)` encapsulates
this check and is shared by both the creator and verifier path tests, keeping each test
focused on the setup path that distinguishes them.

**What is NOT tested here:** The tests do not assert on auth entries from `stake` or
`check_in` because those calls happen in setup before the `env.auths()` snapshot is taken.
`env.auths()` only reflects the most recent invocation.

### Deployment

Deploy the contract to Soroban testnet or mainnet using the Soroban CLI:

```bash
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/accountability_vault.wasm \
  --source <your-secret-key> \
  --network <network-passphrase>
```

### Security Considerations

1. **CEI Pattern**: All token transfers occur after state is persisted to storage.
2. **Emergency Pause**: Guardian can halt settlement paths during disputes.
3. **M-of-N Verification**: No single verifier can unilaterally release funds when
   `approval_threshold > 1`.
4. **Overflow Protection**: Milestone amount summation uses safe integer arithmetic.
5. **Input Validation**: All amounts validated for positivity; milestone amounts must sum
   exactly to the vault amount.
6. **Authorized Operations**: Creator, verifier set, guardian, and oracle roles are
   enforced via `Address::require_auth()`.

### Residual Sweep (reclaim_after_settlement)

The contract exposes `reclaim_after_settlement(token_address)` to sweep any residual token
balance (dust or rounding remainders) held by the contract back to the vault creator.

Requirements:

- Caller must be the vault `creator` (authorization enforced via `require_auth`).
- The vault must have no staked funds remaining (`staked == 0`); otherwise
  `Error::StakedRemaining` is returned.

The function queries the contract's token balance via `token::Client::balance` and performs
a `token::Client::transfer` of the full balance to the creator.

Location: `accountability_vault/src/lib.rs` — `AccountabilityVault::reclaim_after_settlement`

### License

See main repository license file.

## Accountability Vault - Key Behaviors

### Timestamp Boundary Rules
- `slash_on_miss`: 
  - `env.ledger().timestamp() <= vault.end_timestamp` → Returns `DeadlineNotReached` (exact equality is **rejected**)
  - `env.ledger().timestamp() > vault.end_timestamp` → Slash is executed
- `check_in`:
  - Exact equality (`timestamp == milestone.due_date`) is **allowed** and succeeds.

### Check-in Idempotency (#502)
- Calling `check_in` multiple times on the same milestone index returns `MilestoneAlreadyVerified` error on subsequent calls.
- This behavior is **intentional** and expected by the backend (`eventParser.ts`).

These rules are enforced through boundary and idempotency tests in `contracts/accountability_vault/src/test.rs`.
