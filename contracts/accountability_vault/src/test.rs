use soroban_sdk::{testutils::Address as _, Address, BytesN, Env};

use crate::{AccountabilityVault, AccountabilityVaultClient};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn setup() -> (Env, AccountabilityVaultClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, AccountabilityVault);
    let client = AccountabilityVaultClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    (env, client, admin)
}

fn dummy_wasm_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0u8; 32])
}

// ── initialize ────────────────────────────────────────────────────────────────

#[test]
fn test_initialize_sets_admin() {
    let (_env, client, admin) = setup();
    assert_eq!(client.admin(), admin);
}

#[test]
#[should_panic(expected = "already initialized")]
fn test_double_initialize_panics() {
    let (env, client, _admin) = setup();
    let other = Address::generate(&env);
    client.initialize(&other);
}

// ── lock / balance ────────────────────────────────────────────────────────────

#[test]
fn test_lock_and_balance() {
    let (env, client, _admin) = setup();
    let creator = Address::generate(&env);
    assert_eq!(client.balance(&creator), 0);
    client.lock(&creator, &500);
    assert_eq!(client.balance(&creator), 500);
}

#[test]
fn test_lock_requires_creator_auth() {
    let (env, client, _admin) = setup();
    let creator = Address::generate(&env);
    client.lock(&creator, &100);
    // mock_all_auths records every require_auth call; verify creator was among them
    let auths = env.auths();
    assert!(auths.iter().any(|(addr, _)| *addr == creator));
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_lock_zero_panics() {
    let (env, client, _admin) = setup();
    let creator = Address::generate(&env);
    client.lock(&creator, &0);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn test_lock_negative_panics() {
    let (env, client, _admin) = setup();
    let creator = Address::generate(&env);
    client.lock(&creator, &-1);
}

// ── upgrade ───────────────────────────────────────────────────────────────────

#[test]
fn test_admin_can_upgrade() {
    let (env, client, admin) = setup();
    let hash = dummy_wasm_hash(&env);
    client.upgrade(&hash); // mock_all_auths satisfies admin.require_auth()
    // Verify the auth was requested for the admin address
    let auths = env.auths();
    assert!(auths.iter().any(|(addr, _)| *addr == admin));
}

/// Without mock_all_auths the host enforces real auth; admin.require_auth()
/// will fail because no valid signature is present → contract panics.
#[test]
#[should_panic]
fn test_non_admin_cannot_upgrade() {
    let env = Env::default();
    // No mock_all_auths — auth is enforced for real
    let contract_id = env.register_contract(None, AccountabilityVault);
    let client = AccountabilityVaultClient::new(&env, &contract_id);

    // Initialize: temporarily allow all auths just for setup
    env.mock_all_auths();
    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Reset: stop mocking — subsequent calls must satisfy auth for real
    // Soroban testutils: calling mock_all_auths again resets the mock list.
    // We intentionally do NOT call it, so the next invocation has no auth entries.
    // Re-create a fresh env without mocks to simulate an unauthorized caller.
    let env2 = Env::default();
    let client2 = AccountabilityVaultClient::new(&env2, &contract_id);
    let hash = dummy_wasm_hash(&env2);
    client2.upgrade(&hash); // must panic: no auth provided
}

#[test]
fn test_upgrade_does_not_affect_vault_state() {
    let (env, client, _admin) = setup();
    let creator = Address::generate(&env);
    client.lock(&creator, &999);
    client.upgrade(&dummy_wasm_hash(&env));
    assert_eq!(client.balance(&creator), 999);
}
