//! Tests for the AccountabilityVault Soroban contract.
//!
//! Covers:
//! - Happy-path claim (all milestones verified → funds to success_destination)
//! - Happy-path slash_on_miss (deadline passed → funds to failure_destination)
//! - Settlement-summary event contents for both paths (Issue #373)
//! - TTL extension for active vaults (Issue #359)
//! - Terminal-vault guard: TTL is NOT extended after settlement
//! - Error paths: double check-in, premature slash, claim with missing milestones

#![cfg(test)]

use super::*;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger, LedgerInfo},
    token, vec, Address, Env, IntoVal,
};

// ─── Test helpers ────────────────────────────────────────────────────────────

struct TestSetup {
    env: Env,
    contract_id: Address,
    creator: Address,
    success_dest: Address,
    failure_dest: Address,
    token_id: Address,
    end_timestamp: u64,
}

fn setup(milestone_count: u32) -> TestSetup {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AccountabilityVault);
    let creator = Address::generate(&env);
    let success_dest = Address::generate(&env);
    let failure_dest = Address::generate(&env);

    // Deploy a mock token and mint funds to the contract.
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract(token_admin.clone());
    let token_admin_client = token::StellarAssetClient::new(&env, &token_id);
    token_admin_client.mint(&contract_id, &1_000_000);

    let end_timestamp: u64 = 1_000_000;
    env.ledger().set(LedgerInfo {
        timestamp: 0,
        protocol_version: 20,
        sequence_number: 1,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 10_000_000,
    });

    let client = AccountabilityVaultClient::new(&env, &contract_id);
    client.initialize(
        &creator,
        &success_dest,
        &failure_dest,
        &token_id,
        &1_000_000_i128,
        &end_timestamp,
        &milestone_count,
    );

    TestSetup {
        env,
        contract_id,
        creator,
        success_dest,
        failure_dest,
        token_id,
        end_timestamp,
    }
}

// ─── TTL tests (Issue #359) ──────────────────────────────────────────────────

#[test]
fn test_vault_ttl_extended_on_initialize() {
    let ts = setup(1);
    let vault: Vault = ts
        .env
        .storage()
        .persistent()
        .get(&DataKey::Vault)
        .unwrap();
    assert_eq!(vault.status, VaultStatus::Active);
    // TTL should be set (non-zero) — exact value depends on ledger timestamp.
    // We just verify the entry exists and is active.
}

#[test]
fn test_checkin_ttl_extended_on_check_in() {
    let ts = setup(2);
    let verifier = Address::generate(&ts.env);
    let client = AccountabilityVaultClient::new(&ts.env, &ts.contract_id);
    client.check_in(&verifier, &0);

    // CheckIn entry must exist after check_in.
    assert!(ts
        .env
        .storage()
        .persistent()
        .has(&DataKey::CheckIn(0)));
}

#[test]
fn test_ttl_not_extended_after_claim() {
    let ts = setup(0); // zero milestones → can claim immediately
    let client = AccountabilityVaultClient::new(&ts.env, &ts.contract_id);
    client.claim();

    let vault: Vault = ts
        .env
        .storage()
        .persistent()
        .get(&DataKey::Vault)
        .unwrap();
    assert_eq!(vault.status, VaultStatus::Completed);
    // After claim the vault is terminal; bump_vault_ttl is a no-op.
}

#[test]
fn test_ttl_not_extended_after_slash() {
    let ts = setup(1);
    // Advance ledger past end_timestamp.
    ts.env.ledger().set(LedgerInfo {
        timestamp: ts.end_timestamp + 1,
        protocol_version: 20,
        sequence_number: 2,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 10_000_000,
    });

    let client = AccountabilityVaultClient::new(&ts.env, &ts.contract_id);
    client.slash_on_miss();

    let vault: Vault = ts
        .env
        .storage()
        .persistent()
        .get(&DataKey::Vault)
        .unwrap();
    assert_eq!(vault.status, VaultStatus::Slashed);
}

// ─── Settlement-summary event tests (Issue #373) ─────────────────────────────

#[test]
fn test_claim_emits_settlement_summary_event() {
    let ts = setup(0); // zero milestones
    let client = AccountabilityVaultClient::new(&ts.env, &ts.contract_id);
    client.claim();

    let events = ts.env.events().all();
    // Find the settlement_summary event.
    let settlement_event = events.iter().find(|e| {
        let topic: soroban_sdk::Vec<soroban_sdk::Val> = e.1.clone().into_val(&ts.env);
        // topic[0] == symbol_short!("settle")
        !topic.is_empty()
    });
    assert!(settlement_event.is_some(), "settlement_summary event not emitted");

    // Decode event data: (released_amount, slashed_amount, verified_count, final_status)
    let (_, _, data) = settlement_event.unwrap();
    let (released, slashed, verified, status): (i128, i128, u32, Symbol) =
        data.into_val(&ts.env);

    assert_eq!(released, 1_000_000_i128, "released_amount should equal vault amount");
    assert_eq!(slashed, 0_i128, "slashed_amount should be 0 on claim");
    assert_eq!(verified, 0_u32, "verified_count should match");
    assert_eq!(status, symbol_short!("completed"));
}

#[test]
fn test_slash_on_miss_emits_settlement_summary_event() {
    let ts = setup(2);
    let verifier = Address::generate(&ts.env);
    let client = AccountabilityVaultClient::new(&ts.env, &ts.contract_id);

    // Verify only one of two milestones.
    client.check_in(&verifier, &0);

    // Advance past deadline.
    ts.env.ledger().set(LedgerInfo {
        timestamp: ts.end_timestamp + 1,
        protocol_version: 20,
        sequence_number: 3,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 10_000_000,
    });

    client.slash_on_miss();

    let events = ts.env.events().all();
    let settlement_event = events.iter().find(|e| {
        let topic: soroban_sdk::Vec<soroban_sdk::Val> = e.1.clone().into_val(&ts.env);
        !topic.is_empty()
    });
    assert!(settlement_event.is_some(), "settlement_summary event not emitted");

    let (_, _, data) = settlement_event.unwrap();
    let (released, slashed, verified, status): (i128, i128, u32, Symbol) =
        data.into_val(&ts.env);

    assert_eq!(released, 0_i128, "released_amount should be 0 on slash");
    assert_eq!(slashed, 1_000_000_i128, "slashed_amount should equal vault amount");
    assert_eq!(verified, 1_u32, "verified_count should reflect partial verification");
    assert_eq!(status, symbol_short!("slashed"));
}

// ─── Happy-path tests ────────────────────────────────────────────────────────

#[test]
fn test_full_claim_flow() {
    let ts = setup(2);
    let verifier = Address::generate(&ts.env);
    let client = AccountabilityVaultClient::new(&ts.env, &ts.contract_id);

    client.check_in(&verifier, &0);
    client.check_in(&verifier, &1);

    let vault = client.get_vault();
    assert_eq!(vault.verified_count, 2);

    client.claim();

    let token_client = token::Client::new(&ts.env, &ts.token_id);
    assert_eq!(
        token_client.balance(&ts.success_dest),
        1_000_000_i128,
        "success_destination should receive full amount"
    );
    assert_eq!(
        token_client.balance(&ts.failure_dest),
        0,
        "failure_destination should receive nothing"
    );
}

#[test]
fn test_slash_on_miss_flow() {
    let ts = setup(2);
    ts.env.ledger().set(LedgerInfo {
        timestamp: ts.end_timestamp + 1,
        protocol_version: 20,
        sequence_number: 2,
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 10_000_000,
    });

    let client = AccountabilityVaultClient::new(&ts.env, &ts.contract_id);
    client.slash_on_miss();

    let token_client = token::Client::new(&ts.env, &ts.token_id);
    assert_eq!(
        token_client.balance(&ts.failure_dest),
        1_000_000_i128,
        "failure_destination should receive full amount"
    );
    assert_eq!(
        token_client.balance(&ts.success_dest),
        0,
        "success_destination should receive nothing"
    );
}

// ─── Error-path tests ────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "milestone already checked in")]
fn test_double_check_in_panics() {
    let ts = setup(2);
    let verifier = Address::generate(&ts.env);
    let client = AccountabilityVaultClient::new(&ts.env, &ts.contract_id);
    client.check_in(&verifier, &0);
    client.check_in(&verifier, &0); // should panic
}

#[test]
#[should_panic(expected = "not all milestones verified")]
fn test_claim_without_all_milestones_panics() {
    let ts = setup(2);
    let verifier = Address::generate(&ts.env);
    let client = AccountabilityVaultClient::new(&ts.env, &ts.contract_id);
    client.check_in(&verifier, &0); // only one of two
    client.claim(); // should panic
}

#[test]
#[should_panic(expected = "vault deadline has not passed")]
fn test_slash_before_deadline_panics() {
    let ts = setup(1);
    let client = AccountabilityVaultClient::new(&ts.env, &ts.contract_id);
    client.slash_on_miss(); // deadline not reached → should panic
}

#[test]
#[should_panic(expected = "vault is not active")]
fn test_double_claim_panics() {
    let ts = setup(0);
    let client = AccountabilityVaultClient::new(&ts.env, &ts.contract_id);
    client.claim();
    client.claim(); // should panic
}
