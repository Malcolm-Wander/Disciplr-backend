extern crate std;

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{token, vec, Address, Env};

fn vault_id(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[7; 32])
}

fn setup(env: &Env) -> (AccountabilityVaultClient<'_>, Address, Address, Address, Address, Address) {
    env.mock_all_auths();

    let contract_id = env.register_contract(None, AccountabilityVault);
    let client = AccountabilityVaultClient::new(env, &contract_id);

    let token_admin = Address::generate(env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_address = token_id.address();
    let creator = Address::generate(env);
    let verifier = Address::generate(env);
    let success = Address::generate(env);
    let failure = Address::generate(env);

    let token_admin_client = token::StellarAssetClient::new(env, &token_address);
    token_admin_client.mint(&creator, &1_000);

    (client, token_address, creator, verifier, success, failure)
}

#[test]
fn mixed_outcome_claim_pays_verified_and_slashes_unverified_after_deadline() {
    let env = Env::default();
    let (client, token_address, creator, verifier, success, failure) = setup(&env);
    let token_client = token::Client::new(&env, &token_address);
    let id = vault_id(&env);

    client.create_vault(
        &id,
        &token_address,
        &creator,
        &verifier,
        &success,
        &failure,
        &1_000,
        &100,
        &vec![&env, 300_i128, 700_i128],
    );
    client.verify_milestone(&id, &0);
    env.ledger().set_timestamp(101);

    let settlement = client.claim(&id);

    assert_eq!(settlement.success_amount, 300);
    assert_eq!(settlement.failure_amount, 700);
    assert_eq!(settlement.success_amount + settlement.failure_amount, 1_000);
    assert_eq!(token_client.balance(&success), 300);
    assert_eq!(token_client.balance(&failure), 700);
    assert_eq!(token_client.balance(&client.address), 0);
}

#[test]
fn mixed_outcome_slash_on_miss_uses_the_same_proportional_split() {
    let env = Env::default();
    let (client, token_address, creator, verifier, success, failure) = setup(&env);
    let token_client = token::Client::new(&env, &token_address);
    let id = vault_id(&env);

    client.create_vault(
        &id,
        &token_address,
        &creator,
        &verifier,
        &success,
        &failure,
        &1_000,
        &100,
        &vec![&env, 250_i128, 250_i128, 500_i128],
    );
    client.verify_milestone(&id, &0);
    client.verify_milestone(&id, &2);
    env.ledger().set_timestamp(100);

    let settlement = client.slash_on_miss(&id);

    assert_eq!(settlement.success_amount, 750);
    assert_eq!(settlement.failure_amount, 250);
    assert_eq!(settlement.success_amount + settlement.failure_amount, 1_000);
    assert_eq!(token_client.balance(&success), 750);
    assert_eq!(token_client.balance(&failure), 250);
    assert_eq!(token_client.balance(&client.address), 0);
}

#[test]
fn all_verified_sends_the_full_stake_to_success_destination() {
    let env = Env::default();
    let (client, token_address, creator, verifier, success, failure) = setup(&env);
    let token_client = token::Client::new(&env, &token_address);
    let id = vault_id(&env);

    client.create_vault(
        &id,
        &token_address,
        &creator,
        &verifier,
        &success,
        &failure,
        &1_000,
        &100,
        &vec![&env, 400_i128, 600_i128],
    );
    client.verify_milestone(&id, &0);
    client.verify_milestone(&id, &1);
    env.ledger().set_timestamp(101);

    let settlement = client.claim(&id);

    assert_eq!(settlement.success_amount, 1_000);
    assert_eq!(settlement.failure_amount, 0);
    assert_eq!(token_client.balance(&success), 1_000);
    assert_eq!(token_client.balance(&failure), 0);
    assert_eq!(token_client.balance(&client.address), 0);
}

#[test]
fn none_verified_slashes_the_full_stake_to_failure_destination() {
    let env = Env::default();
    let (client, token_address, creator, verifier, success, failure) = setup(&env);
    let token_client = token::Client::new(&env, &token_address);
    let id = vault_id(&env);

    client.create_vault(
        &id,
        &token_address,
        &creator,
        &verifier,
        &success,
        &failure,
        &1_000,
        &100,
        &vec![&env, 300_i128, 700_i128],
    );
    env.ledger().set_timestamp(101);

    let settlement = client.slash_on_miss(&id);

    assert_eq!(settlement.success_amount, 0);
    assert_eq!(settlement.failure_amount, 1_000);
    assert_eq!(token_client.balance(&success), 0);
    assert_eq!(token_client.balance(&failure), 1_000);
    assert_eq!(token_client.balance(&client.address), 0);
}

#[test]
#[should_panic(expected = "MilestoneSumMismatch")]
fn create_vault_rejects_milestones_that_do_not_equal_the_stake() {
    let env = Env::default();
    let (client, token_address, creator, verifier, success, failure) = setup(&env);

    client.create_vault(
        &vault_id(&env),
        &token_address,
        &creator,
        &verifier,
        &success,
        &failure,
        &1_000,
        &100,
        &vec![&env, 499_i128, 500_i128],
    );
}

#[test]
#[should_panic(expected = "DeadlineNotPassed")]
fn settlement_requires_the_deadline_to_pass() {
    let env = Env::default();
    let (client, token_address, creator, verifier, success, failure) = setup(&env);

    client.create_vault(
        &vault_id(&env),
        &token_address,
        &creator,
        &verifier,
        &success,
        &failure,
        &1_000,
        &100,
        &vec![&env, 1_000_i128],
    );
    env.ledger().set_timestamp(99);

    client.claim(&vault_id(&env));
}

#[test]
#[should_panic(expected = "AlreadySettled")]
fn settlement_is_single_use() {
    let env = Env::default();
    let (client, token_address, creator, verifier, success, failure) = setup(&env);
    let id = vault_id(&env);

    client.create_vault(
        &id,
        &token_address,
        &creator,
        &verifier,
        &success,
        &failure,
        &1_000,
        &100,
        &vec![&env, 1_000_i128],
    );
    env.ledger().set_timestamp(101);

    client.slash_on_miss(&id);
    client.claim(&id);
}

