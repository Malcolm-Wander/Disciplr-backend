//! Accountability Vault — Soroban smart contract
//!
//! Manages time-locked capital vaults on Stellar. Funds are released to
//! `success_destination` on `claim` (all milestones verified) or swept to
//! `failure_destination` on `slash_on_miss` (deadline passed without
//! verification).
//!
//! ## Storage TTL (Issue #359)
//! Every write/read of an active vault bumps the storage TTL so that entries
//! survive until at least `end_timestamp`. Terminal vaults (completed /
//! slashed) are **not** extended — they can be archived once settled.
//!
//! ## Settlement-summary event (Issue #373)
//! Both `claim` and `slash_on_miss` emit a `settlement_summary` event
//! containing `released_amount`, `slashed_amount`, `verified_count`, and
//! `final_status`. The topic name is `settlement_summary`, which matches the
//! `EventType` union in `src/types/horizonSync.ts`.

#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, token, Address, Env, Symbol,
};

// ─── Storage keys ────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Vault,
    CheckIn(u32), // keyed by milestone index
}

// ─── Domain types ────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, PartialEq)]
pub enum VaultStatus {
    Active,
    Completed,
    Slashed,
}

#[contracttype]
#[derive(Clone)]
pub struct Vault {
    pub creator: Address,
    pub success_destination: Address,
    pub failure_destination: Address,
    pub token: Address,
    pub amount: i128,
    pub end_timestamp: u64,
    pub verified_count: u32,
    pub milestone_count: u32,
    pub status: VaultStatus,
}

#[contracttype]
#[derive(Clone)]
pub struct CheckIn {
    pub milestone_index: u32,
    pub verified: bool,
    pub verified_at: u64,
}

// ─── TTL helpers ─────────────────────────────────────────────────────────────

/// Ledgers-per-second approximation on Stellar (5 s/ledger).
const LEDGERS_PER_SECOND: u64 = 5;

/// Minimum TTL extension in ledgers (≈ 1 day).
const MIN_TTL_LEDGERS: u32 = 17_280;

/// Compute the number of ledgers from now until `end_timestamp`, clamped to
/// at least `MIN_TTL_LEDGERS`.
fn ttl_ledgers_until(env: &Env, end_timestamp: u64) -> u32 {
    let now = env.ledger().timestamp();
    if end_timestamp <= now {
        return MIN_TTL_LEDGERS;
    }
    let seconds_remaining = end_timestamp - now;
    let ledgers = (seconds_remaining / LEDGERS_PER_SECOND) as u32;
    ledgers.max(MIN_TTL_LEDGERS)
}

/// Bump the TTL for the `Vault` entry if the vault is still active.
fn bump_vault_ttl(env: &Env, vault: &Vault) {
    if vault.status != VaultStatus::Active {
        return;
    }
    let ttl = ttl_ledgers_until(env, vault.end_timestamp);
    env.storage()
        .persistent()
        .extend_ttl(&DataKey::Vault, ttl, ttl);
}

/// Bump the TTL for a `CheckIn` entry if the vault is still active.
fn bump_checkin_ttl(env: &Env, vault: &Vault, index: u32) {
    if vault.status != VaultStatus::Active {
        return;
    }
    let ttl = ttl_ledgers_until(env, vault.end_timestamp);
    env.storage()
        .persistent()
        .extend_ttl(&DataKey::CheckIn(index), ttl, ttl);
}

// ─── Event helpers ───────────────────────────────────────────────────────────

/// Emit a `settlement_summary` event.
///
/// Topic: `["settlement_summary"]`
/// Data:  `(released_amount, slashed_amount, verified_count, final_status)`
///
/// The topic name matches `EventType = 'settlement_summary'` in
/// `src/types/horizonSync.ts` so the ETL pipeline can ingest it without
/// re-querying the ledger.
fn emit_settlement_summary(
    env: &Env,
    released_amount: i128,
    slashed_amount: i128,
    verified_count: u32,
    final_status: Symbol,
) {
    let topic = (symbol_short!("settle"),);
    let data = (released_amount, slashed_amount, verified_count, final_status);
    env.events().publish(topic, data);
}

// ─── Contract ────────────────────────────────────────────────────────────────

#[contract]
pub struct AccountabilityVault;

#[contractimpl]
impl AccountabilityVault {
    /// Initialise a new vault.
    ///
    /// Stores the `Vault` entry and bumps its TTL to survive until
    /// `end_timestamp`.
    pub fn initialize(
        env: Env,
        creator: Address,
        success_destination: Address,
        failure_destination: Address,
        token: Address,
        amount: i128,
        end_timestamp: u64,
        milestone_count: u32,
    ) {
        creator.require_auth();

        let vault = Vault {
            creator,
            success_destination,
            failure_destination,
            token,
            amount,
            end_timestamp,
            verified_count: 0,
            milestone_count,
            status: VaultStatus::Active,
        };

        env.storage().persistent().set(&DataKey::Vault, &vault);
        bump_vault_ttl(&env, &vault);
    }

    /// Record a milestone check-in (verified by the assigned verifier).
    ///
    /// Bumps TTL for both the `Vault` and the new `CheckIn` entry.
    pub fn check_in(env: Env, verifier: Address, milestone_index: u32) {
        verifier.require_auth();

        let mut vault: Vault = env
            .storage()
            .persistent()
            .get(&DataKey::Vault)
            .expect("vault not initialised");

        assert!(vault.status == VaultStatus::Active, "vault is not active");
        assert!(
            milestone_index < vault.milestone_count,
            "milestone index out of range"
        );

        let key = DataKey::CheckIn(milestone_index);
        assert!(
            !env.storage().persistent().has(&key),
            "milestone already checked in"
        );

        let check_in = CheckIn {
            milestone_index,
            verified: true,
            verified_at: env.ledger().timestamp(),
        };

        env.storage().persistent().set(&key, &check_in);
        vault.verified_count += 1;
        env.storage().persistent().set(&DataKey::Vault, &vault);

        // Bump TTL for active vault entries.
        bump_vault_ttl(&env, &vault);
        bump_checkin_ttl(&env, &vault, milestone_index);
    }

    /// Claim the vault: transfer funds to `success_destination`.
    ///
    /// Requires all milestones to be verified. Emits a `settlement_summary`
    /// event so the analytics ETL can compute success-rate metrics without
    /// re-querying the ledger.
    ///
    /// Terminal vaults are **not** TTL-extended after settlement.
    pub fn claim(env: Env) {
        let mut vault: Vault = env
            .storage()
            .persistent()
            .get(&DataKey::Vault)
            .expect("vault not initialised");

        assert!(vault.status == VaultStatus::Active, "vault is not active");
        assert!(
            vault.verified_count >= vault.milestone_count,
            "not all milestones verified"
        );

        // Transfer full amount to success destination.
        let token_client = token::Client::new(&env, &vault.token);
        token_client.transfer(
            &env.current_contract_address(),
            &vault.success_destination,
            &vault.amount,
        );

        vault.status = VaultStatus::Completed;
        env.storage().persistent().set(&DataKey::Vault, &vault);

        // Emit settlement-summary event (Issue #373).
        emit_settlement_summary(
            &env,
            vault.amount,   // released_amount
            0,              // slashed_amount
            vault.verified_count,
            symbol_short!("completed"),
        );
    }

    /// Slash on miss: transfer funds to `failure_destination`.
    ///
    /// Can only be called after `end_timestamp` has passed. Emits a
    /// `settlement_summary` event for analytics ingestion.
    ///
    /// Terminal vaults are **not** TTL-extended after settlement.
    pub fn slash_on_miss(env: Env) {
        let mut vault: Vault = env
            .storage()
            .persistent()
            .get(&DataKey::Vault)
            .expect("vault not initialised");

        assert!(vault.status == VaultStatus::Active, "vault is not active");
        assert!(
            env.ledger().timestamp() >= vault.end_timestamp,
            "vault deadline has not passed"
        );

        // Transfer full amount to failure destination.
        let token_client = token::Client::new(&env, &vault.token);
        token_client.transfer(
            &env.current_contract_address(),
            &vault.failure_destination,
            &vault.amount,
        );

        vault.status = VaultStatus::Slashed;
        env.storage().persistent().set(&DataKey::Vault, &vault);

        // Emit settlement-summary event (Issue #373).
        emit_settlement_summary(
            &env,
            0,              // released_amount
            vault.amount,   // slashed_amount
            vault.verified_count,
            symbol_short!("slashed"),
        );
    }

    /// Read the current vault state (bumps TTL if still active).
    pub fn get_vault(env: Env) -> Vault {
        let vault: Vault = env
            .storage()
            .persistent()
            .get(&DataKey::Vault)
            .expect("vault not initialised");
        bump_vault_ttl(&env, &vault);
        vault
    }

    /// Read a check-in entry (bumps TTL if vault is still active).
    pub fn get_check_in(env: Env, milestone_index: u32) -> CheckIn {
        let vault: Vault = env
            .storage()
            .persistent()
            .get(&DataKey::Vault)
            .expect("vault not initialised");

        let check_in: CheckIn = env
            .storage()
            .persistent()
            .get(&DataKey::CheckIn(milestone_index))
            .expect("check-in not found");

        bump_checkin_ttl(&env, &vault, milestone_index);
        check_in
    }
}

mod test;
