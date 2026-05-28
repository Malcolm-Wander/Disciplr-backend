# Disciplr Smart Contracts

Soroban smart contracts for the Disciplr accountability vault system on Stellar.

## accountability_vault

A time-locked capital vault that releases funds to a `success_destination` when
all milestones are verified, or sweeps them to a `failure_destination` when the
deadline passes without full verification.

### Entry points

| Function | Description |
|---|---|
| `initialize` | Create a new vault with creator, destinations, token, amount, deadline, and milestone count |
| `check_in(verifier, milestone_index)` | Record a verified milestone (verifier must sign) |
| `claim()` | Release funds to `success_destination` — requires all milestones verified |
| `slash_on_miss()` | Sweep funds to `failure_destination` — requires deadline passed |
| `get_vault()` | Read current vault state |
| `get_check_in(milestone_index)` | Read a check-in entry |

### Storage TTL (Issue #359)

Soroban persistent storage entries are subject to archival if their TTL expires.
For long-running vaults this is a risk: a vault created months before its
`end_timestamp` could be archived before settlement.

**Strategy:** every write and read of an active vault bumps the TTL of the
`Vault` and `CheckIn` entries to at least `end_timestamp` (computed as
`(end_timestamp - now) / 5 ledgers-per-second`, clamped to a minimum of
`MIN_TTL_LEDGERS = 17_280` ≈ 1 day).

Terminal vaults (`Completed` / `Slashed`) are **not** extended — they can be
archived once settled.

**Operator note:** if a vault's `end_timestamp` is more than ~6 months in the
future, operators should monitor the Stellar network's `max_entry_ttl` parameter
and call `get_vault()` periodically to keep the entry alive.

### Settlement-summary event (Issue #373)

Both `claim` and `slash_on_miss` emit a `settlement_summary` event so the
backend ETL pipeline (`src/services/etlWorker.ts`) can compute success-rate
analytics without re-querying the ledger.

**Topic:** `["settle"]`

**Data:** `(released_amount: i128, slashed_amount: i128, verified_count: u32, final_status: Symbol)`

| Field | `claim` | `slash_on_miss` |
|---|---|---|
| `released_amount` | vault amount | `0` |
| `slashed_amount` | `0` | vault amount |
| `verified_count` | milestone_count | partial count |
| `final_status` | `"completed"` | `"slashed"` |

The event type `settlement_summary` is registered in
`src/types/horizonSync.ts` (`EventType` union) so `src/services/eventParser.ts`
can route it to the analytics pipeline.

### Building

```bash
cd contracts/accountability_vault
cargo build --target wasm32-unknown-unknown --release
```

### Testing

```bash
cd contracts/accountability_vault
cargo test
```
