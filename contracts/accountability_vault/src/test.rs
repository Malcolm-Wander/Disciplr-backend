use soroban_sdk::{Env, Vec};

use crate::{AccountabilityVaultContract, ContractError, MAX_MILESTONES, Milestone};

#[test]
fn create_vault_rejects_more_than_max_milestones() {
    let env = Env::default();
    let mut milestones = Vec::new(&env);
    for _ in 0..(MAX_MILESTONES + 1) {
        milestones.push_back(Milestone { verified: false });
    }

    let result = AccountabilityVaultContract::create_vault(env.clone(), milestones);
    assert_eq!(result, Err(ContractError::TooManyMilestones));
}

#[test]
fn create_vault_allows_max_milestones() {
    let env = Env::default();
    let mut milestones = Vec::new(&env);
    for _ in 0..MAX_MILESTONES {
        milestones.push_back(Milestone { verified: false });
    }

    let result = AccountabilityVaultContract::create_vault(env.clone(), milestones);
    assert!(result.is_ok());
#![cfg(test)]

extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, vec, Address, Env, String, Symbol,
};
use serde_json::json;
use std::fs;

/// Creates a deterministic 32-byte evidence hash for use in tests.
fn evidence_hash(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

fn create_token(env: &Env, admin: &Address) -> (Address, token::StellarAssetClient<'static>) {
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let address = sac.address();
    (
        address.clone(),
        token::StellarAssetClient::new(env, &address),
    )
}

struct Setup {
    env: Env,
    contract: AccountabilityVaultClient<'static>,
    token: Address,
    token_admin: Address,
    #[allow(dead_code)]
    token_admin_client: token::StellarAssetClient<'static>,
    creator: Address,
    verifier: Address,
    guardian: Address,
    success: Address,
    failure: Address,
    vault_id: String,
}

fn setup(milestone_due_offsets: &[u64], amounts: &[i128]) -> Setup {
    setup_with_oracle(milestone_due_offsets, amounts, None)
}

fn setup_with_oracle(
    milestone_due_offsets: &[u64],
    amounts: &[i128],
    oracle: Option<Address>,
) -> Setup {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let creator = Address::generate(&env);
    let verifier = Address::generate(&env);
    let guardian = Address::generate(&env);
    let success = Address::generate(&env);
    let failure = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let (token, token_admin_client) = create_token(&env, &token_admin);
    let total: i128 = amounts.iter().sum();
    token_admin_client.mint(&creator, &total);

    let contract_id = env.register_contract(None, AccountabilityVault);
    let contract = AccountabilityVaultClient::new(&env, &contract_id);

    let mut milestones = vec![&env];
    for (i, due) in milestone_due_offsets.iter().enumerate() {
        milestones.push_back(Milestone {
            title: String::from_str(&env, "m"),
            amount: amounts[i],
            due_date: 1_000 + due,
            verified: false,
            released: false,
        });
    }

    let end = 1_000 + milestone_due_offsets.iter().max().copied().unwrap_or(0);
    let verifier_set = VerifierSet {
        verifiers: vec![&env, verifier.clone()],
        threshold: 1u32,
    };
    contract.create_vault(
        &vault_id,
        &creator,
        &verifier_set,
        &oracle,
        &token,
        &total,
        &success,
        &failure,
        &end,
        &milestones,
        &guardian,
    );

    Setup {
        env,
        contract,
        token,
        token_admin,
        token_admin_client,
        creator,
        verifier,
        guardian,
        success,
        failure,
        vault_id,
    }
}

fn assert_token_admin_balance_unchanged(
    token_client: &token::Client<'_>,
    token_admin: &Address,
    expected_balance: i128,
) {
    assert_eq!(
        token_client.balance(token_admin),
        expected_balance,
        "token admin balance must remain unchanged by vault lifecycle operations",
    );
}

fn assert_stake_rejected_with_not_draft(s: &Setup) {
    let result = s.contract.try_stake(&s.vault_id, &s.creator);
    assert!(
        matches!(result, Err(Ok(Error::NotDraft))),
        "stake after cancellation must be rejected with Error::NotDraft, got: {:?}",
        result,
    );
}

// ── existing lifecycle tests ─────────────────────────────────────────────────

#[test]
fn test_create_and_stake() {
    let s = setup(&[100], &[500]);
    let vault = s.contract.get_vault(&s.vault_id);
    assert_eq!(vault.status, VaultStatus::Draft);

    s.contract.stake(&s.vault_id, &s.creator);
    let vault = s.contract.get_vault(&s.vault_id);
    assert_eq!(vault.status, VaultStatus::Active);
    assert_eq!(vault.staked, 500);

    let token_client = token::Client::new(&s.env, &s.token);
    assert_eq!(token_client.balance(&s.creator), 0);
}

#[test]
fn test_abi_spec_snapshot() {
    // Build a stable JSON representation of the contract ABI.
    let spec = json!({
        "name": "AccountabilityVault",
        "functions": [
            {"name":"create_vault","params":["String","Address","VerifierSet","Option<Address>","Address","i128","Address","Address","u64","Vec<Milestone>","Address"],"result":"Result<(), Error>"},
            {"name":"stake","params":["String","Address"],"result":"Result<(), Error>"},
            {"name":"stake_from","params":["String","Address","Address"],"result":"Result<(), Error>"},
            {"name":"check_in","params":["Address","u32","BytesN<32>"],"result":"Result<(), Error>"},
            {"name":"extend_deadline","params":["String","Address","u64"],"result":"Result<(), Error>"},
            {"name":"slash_on_miss","params":[],"result":"Result<(), Error>"},
            {"name":"claim","params":["Address"],"result":"Result<(), Error>"},
            {"name":"claim_milestone","params":["Address","u32"],"result":"Result<(), Error>"},
            {"name":"cancel_vault","params":["String","Address"],"result":"Result<(), Error>"},
            {"name":"withdraw","params":["String","Address"],"result":"Result<(), Error>"},
            {"name":"admin_dispute","params":["String","Address"],"result":"Result<(), Error>"},
            {"name":"admin_resolve","params":["String","Address","VaultStatus"],"result":"Result<(), Error>"},
            {"name":"emergency_pause","params":["Address"],"result":"Result<(), Error>"},
            {"name":"emergency_unpause","params":["Address"],"result":"Result<(), Error>"},
            {"name":"get_vault","params":["String"],"result":"Result<Vault, Error>"},
            {"name":"reclaim_after_settlement","params":["Address"],"result":"Result<(), Error>"},
            {"name":"configure_window","params":["u64"],"result":"()"},
            {"name":"dispute_milestone","params":["String","Address","u32"],"result":"Result<(), Error>"}
        ]
    });

    let pretty = serde_json::to_string_pretty(&spec).unwrap();

    if std::env::var("UPDATE_SOROBAN_SPEC").is_ok() {
        fs::create_dir_all("spec").ok();
        fs::write("spec/AccountabilityVault.spec.json", &pretty).unwrap();
    } else {
        let want = fs::read_to_string("spec/AccountabilityVault.spec.json").expect("snapshot missing; set UPDATE_SOROBAN_SPEC=1 to update");
        assert_eq!(pretty, want);
    }
}

#[test]
fn test_check_in_and_claim_success() {
    let s = setup(&[100, 200], &[300, 700]);
    s.contract.stake(&s.vault_id, &s.creator);

    s.contract.check_in(&s.vault_id, &s.verifier, &0, &evidence_hash(&s.env, 1));
    s.contract.check_in(&s.vault_id, &s.verifier, &1, &evidence_hash(&s.env, 1));

    s.contract.claim(&s.vault_id, &s.creator);
    let vault = s.contract.get_vault(&s.vault_id);
    assert_eq!(vault.status, VaultStatus::Completed);

    let token_client = token::Client::new(&s.env, &s.token);
    assert_eq!(token_client.balance(&s.success), 1000);
}

#[test]
fn test_check_in_out_of_range_returns_typed_error() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);

    let result = s.contract.try_check_in(&s.verifier, &1);

    assert!(matches!(result, Err(Ok(Error::MilestoneIndexOutOfRange))));
}

#[test]
fn test_slash_on_miss() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);

    // Advance past the deadline without any check-in.
    s.env.ledger().set_timestamp(2_000);
    s.contract.slash_on_miss(&s.vault_id);

    let vault = s.contract.get_vault(&s.vault_id);
    assert_eq!(vault.status, VaultStatus::Failed);

    let token_client = token::Client::new(&s.env, &s.token);
    assert_eq!(token_client.balance(&s.failure), 500);
}

#[test]
fn test_token_admin_balance_invariant_success_lifecycle() {
    let s = setup(&[100, 200], &[300, 700]);
    let token_client = token::Client::new(&s.env, &s.token);
    let admin_balance_before = token_client.balance(&s.token_admin);

    // Stake should only move creator -> vault.
    s.contract.stake(&s.vault_id, &s.creator);
    assert_token_admin_balance_unchanged(&token_client, &s.token_admin, admin_balance_before);

    // Check-ins should not move any tokens.
    s.contract
        .check_in(&s.vault_id, &s.verifier, &0, &evidence_hash(&s.env, 7));
    assert_token_admin_balance_unchanged(&token_client, &s.token_admin, admin_balance_before);
    s.contract
        .check_in(&s.vault_id, &s.verifier, &1, &evidence_hash(&s.env, 9));
    assert_token_admin_balance_unchanged(&token_client, &s.token_admin, admin_balance_before);

    // Claim should move vault -> success destination only.
    s.contract.claim(&s.vault_id, &s.creator);
    assert_token_admin_balance_unchanged(&token_client, &s.token_admin, admin_balance_before);
}

#[test]
fn test_token_admin_balance_invariant_slash_lifecycle() {
    let s = setup(&[100], &[500]);
    let token_client = token::Client::new(&s.env, &s.token);
    let admin_balance_before = token_client.balance(&s.token_admin);

    // Stake should only move creator -> vault.
    s.contract.stake(&s.vault_id, &s.creator);
    assert_token_admin_balance_unchanged(&token_client, &s.token_admin, admin_balance_before);

    // Slash should move vault -> failure destination only.
    s.env.ledger().set_timestamp(2_000);
    s.contract.slash_on_miss(&s.vault_id);
    assert_token_admin_balance_unchanged(&token_client, &s.token_admin, admin_balance_before);
}

#[test]
fn test_withdraw_draft_cancels() {
    let s = setup(&[100], &[500]);
    s.contract.cancel_vault(&s.vault_id, &s.creator);
    let vault = s.contract.get_vault(&s.vault_id);
    assert_eq!(vault.status, VaultStatus::Cancelled);
}

#[test]
fn test_cancel_vault_then_stake_rejected_with_not_draft() {
    let s = setup(&[100], &[500]);
    s.contract.cancel_vault(&s.vault_id, &s.creator);
    let vault = s.contract.get_vault(&s.vault_id);
    assert_eq!(vault.status, VaultStatus::Cancelled);

    // Terminal-state regression guard: Cancelled vault must never accept stake.
    assert_stake_rejected_with_not_draft(&s);
}

#[test]
fn test_withdraw_active_refunds_creator() {
    let s = setup(&[100], &[500]);
    // Fund the vault and then call withdraw without any check-ins.
    s.contract.stake(&s.vault_id, &s.creator);

    s.contract.withdraw(&s.vault_id, &s.creator);
    let vault = s.contract.get_vault(&s.vault_id);
    assert_eq!(vault.status, VaultStatus::Cancelled);

    let token_client = token::Client::new(&s.env, &s.token);
    assert_eq!(token_client.balance(&s.creator), 500);
}

#[test]
fn test_withdraw_cancelled_then_stake_rejected_with_not_draft() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);
    s.contract.withdraw(&s.vault_id, &s.creator);

    let vault = s.contract.get_vault(&s.vault_id);
    assert_eq!(vault.status, VaultStatus::Cancelled);

    // Terminal-state regression guard: once cancelled via withdraw, staking is blocked.
    assert_stake_rejected_with_not_draft(&s);
}

#[test]
#[should_panic]
fn test_claim_before_all_verified_fails() {
    let s = setup(&[100, 200], &[300, 700]);
    s.contract.stake(&s.vault_id, &s.creator);
    s.contract.check_in(&s.vault_id, &s.verifier, &0, &evidence_hash(&s.env, 1));
    // Second milestone not yet verified -> claim must fail.
    s.contract.claim(&s.vault_id, &s.creator);
}

#[test]
#[should_panic]
fn test_slash_before_deadline_fails() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);
    s.contract.slash_on_miss(&s.vault_id);
}

// ── issue #368: balance delta assertion in stake ─────────────────────────────

#[test]
fn test_stake_records_balance_delta_as_staked() {
    // For a standard token (no fee on transfer) the delta equals vault.amount.
    let s = setup(&[100], &[800]);
    s.contract.stake(&s.vault_id, &s.creator);
    let vault = s.contract.get_vault(&s.vault_id);
    assert_eq!(vault.staked, 800);
    assert_eq!(vault.status, VaultStatus::Active);
}

#[test]
#[should_panic]
fn test_stake_unauthorized_non_creator_fails() {
    let s = setup(&[100], &[500]);
    let other = Address::generate(&s.env);
    s.contract.stake(&s.vault_id, &other);
}

#[test]
#[should_panic]
fn test_stake_double_stake_fails() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);
    // Second stake on an Active vault must fail with AlreadyStaked / NotDraft.
    s.contract.stake(&s.vault_id, &s.creator);
}

// ── issue #370: stake_from allowance-based variant ───────────────────────────

#[test]
fn test_stake_from_with_sufficient_allowance() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let creator = Address::generate(&env);
    let verifier = Address::generate(&env);
    let guardian = Address::generate(&env);
    let spender = Address::generate(&env);
    let success = Address::generate(&env);
    let failure = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let (token, token_admin_client) = create_token(&env, &token_admin);
    token_admin_client.mint(&creator, &1_000);

    let contract_id = env.register_contract(None, AccountabilityVault);
    let contract = AccountabilityVaultClient::new(&env, &contract_id);

    let verifier_set = VerifierSet {
        verifiers: vec![&env, verifier.clone()],
        threshold: 1u32,
    };
    let milestones = vec![
        &env,
        Milestone {
            title: String::from_str(&env, "m1"),
            amount: 1_000,
            due_date: 1_200,
            verified: false,
            released: false,
        },
    ];
    let vault_id = String::from_str(&env, "v1");
    contract.create_vault(
        &creator, &verifier_set, &None, &token, &1_000, &success, &failure, &1_200,
        &milestones, &guardian,
    );

    // Creator approves spender to spend 1_000 tokens on their behalf.
    let token_client = token::Client::new(&env, &token);
    token_client.approve(&creator, &spender, &1_000, &200);

    contract.stake_from(&vault_id, &creator, &spender);

    let vault = contract.get_vault(&vault_id);
    assert_eq!(vault.status, VaultStatus::Active);
    assert_eq!(vault.staked, 1_000);
    assert_eq!(token_client.balance(&creator), 0);
}

#[test]
#[should_panic]
fn test_stake_from_insufficient_allowance_fails() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let creator = Address::generate(&env);
    let verifier = Address::generate(&env);
    let guardian = Address::generate(&env);
    let spender = Address::generate(&env);
    let success = Address::generate(&env);
    let failure = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let (token, token_admin_client) = create_token(&env, &token_admin);
    token_admin_client.mint(&creator, &1_000);

    let contract_id = env.register_contract(None, AccountabilityVault);
    let contract = AccountabilityVaultClient::new(&env, &contract_id);

    let verifier_set = VerifierSet {
        verifiers: vec![&env, verifier.clone()],
        threshold: 1u32,
    };
    let milestones = vec![
        &env,
        Milestone {
            title: String::from_str(&env, "m1"),
            amount: 1_000,
            due_date: 1_200,
            verified: false,
            released: false,
        },
    ];
    let vault_id = String::from_str(&env, "v1");
    contract.create_vault(
        &creator, &verifier_set, &None, &token, &1_000, &success, &failure, &1_200,
        &milestones, &guardian,
    );

    // Approve only 500 — less than the 1_000 vault amount.
    let token_client = token::Client::new(&env, &token);
    token_client.approve(&creator, &spender, &500, &200);

    // Must fail with InsufficientAllowance.
    contract.stake_from(&vault_id, &creator, &spender);
}

#[test]
#[should_panic]
fn test_stake_from_non_creator_from_fails() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let creator = Address::generate(&env);
    let non_creator = Address::generate(&env);
    let verifier = Address::generate(&env);
    let guardian = Address::generate(&env);
    let spender = Address::generate(&env);
    let success = Address::generate(&env);
    let failure = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let (token, token_admin_client) = create_token(&env, &token_admin);
    token_admin_client.mint(&non_creator, &1_000);

    let contract_id = env.register_contract(None, AccountabilityVault);
    let contract = AccountabilityVaultClient::new(&env, &contract_id);

    let verifier_set = VerifierSet {
        verifiers: vec![&env, verifier.clone()],
        threshold: 1u32,
    };
    let milestones = vec![
        &env,
        Milestone {
            title: String::from_str(&env, "m1"),
            amount: 1_000,
            due_date: 1_200,
            verified: false,
            released: false,
        },
    ];
    let vault_id = String::from_str(&env, "v1");
    contract.create_vault(
        &creator, &verifier_set, &None, &token, &1_000, &success, &failure, &1_200,
        &milestones, &guardian,
    );

    // `from` is not the creator — must be rejected with Unauthorized.
    contract.stake_from(&vault_id, &non_creator, &spender);
}

// ── issue #372: extend_deadline with dual auth ───────────────────────────────

#[test]
fn test_extend_deadline_success() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);

    let vault_before = s.contract.get_vault(&s.vault_id);
    let old_end = vault_before.end_timestamp;

    let new_end = old_end + 500;
    s.contract.extend_deadline(&s.creator, &new_end);

    let vault_after = s.contract.get_vault(&s.vault_id);
    assert_eq!(vault_after.end_timestamp, new_end);
    assert_eq!(vault_after.status, VaultStatus::Active);
}

#[test]
#[should_panic]
fn test_extend_deadline_on_draft_fails() {
    let s = setup(&[100], &[500]);
    // Vault is Draft — extend_deadline must reject with NotActive.
    s.contract.extend_deadline(&s.creator, &2_000);
}

#[test]
#[should_panic]
fn test_extend_deadline_after_deadline_passed_fails() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);

    // Advance past the end_timestamp.
    s.env.ledger().set_timestamp(2_000);
    s.contract.extend_deadline(&s.creator, &3_000);
}

#[test]
#[should_panic]
fn test_extend_deadline_not_greater_than_current_fails() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);

    let vault = s.contract.get_vault(&s.vault_id);
    // Pass the same end_timestamp — must fail with InvalidDeadline.
    s.contract.extend_deadline(&s.creator, &vault.end_timestamp);
}

#[test]
#[should_panic]
fn test_extend_deadline_milestone_exceeds_new_end_fails() {
    // milestone due_date = 1_100, vault end = 1_100.
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);

    // Try to extend to 1_050 — milestone due_date (1_100) > new_end (1_050).
    s.contract.extend_deadline(&s.creator, &1_050);
}

#[test]
#[should_panic]
fn test_extend_deadline_wrong_creator_fails() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);

    let impostor = Address::generate(&s.env);
    s.contract.extend_deadline(&impostor, &2_000);
}

// ── issue #364: verifier threshold validation in create_vault ────────────────

#[test]
#[should_panic]
fn test_create_vault_invalid_threshold_exceeds_verifiers_fails() {
    // threshold=2 with only 1 verifier must fail with InvalidThreshold.
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let creator = Address::generate(&env);
    let verifier = Address::generate(&env);
    let guardian = Address::generate(&env);
    let success = Address::generate(&env);
    let failure = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let (token, _) = create_token(&env, &token_admin);

    let contract_id = env.register_contract(None, AccountabilityVault);
    let contract = AccountabilityVaultClient::new(&env, &contract_id);

    // threshold=2 but only 1 verifier — must fail with InvalidThreshold.
    let verifier_set = VerifierSet {
        verifiers: vec![&env, verifier.clone()],
        threshold: 2u32,
    };
    let milestones = vec![
        &env,
        Milestone {
            title: String::from_str(&env, "m"),
            amount: 500,
            due_date: 1_200,
            verified: false,
        },
    ];
    let vault_id = String::from_str(&env, "v1");
    contract.create_vault(
        &vault_id, &creator, &verifier_set, &None, &token, &500, &success, &failure, &1_200,
        &milestones, &guardian,
    );
}

#[test]
#[should_panic]
fn test_create_vault_zero_threshold_fails() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let creator = Address::generate(&env);
    let verifier = Address::generate(&env);
    let guardian = Address::generate(&env);
    let success = Address::generate(&env);
    let failure = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let (token, _) = create_token(&env, &token_admin);

    let contract_id = env.register_contract(None, AccountabilityVault);
    let contract = AccountabilityVaultClient::new(&env, &contract_id);

    let verifier_set = VerifierSet {
        verifiers: vec![&env, verifier.clone()],
        threshold: 0u32,
    };
    let milestones = vec![
        &env,
        Milestone {
            title: String::from_str(&env, "m"),
            amount: 500,
            due_date: 1_200,
            verified: false,
        },
    ];
    contract.create_vault(
        &creator, &verifier_set, &None, &token, &500, &success, &failure, &1_200,
        &milestones, &guardian,
    );
}

#[test]
fn test_create_vault_zero_amount_after_positives_fails() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let creator = Address::generate(&env);
    let verifier = Address::generate(&env);
    let guardian = Address::generate(&env);
    let success = Address::generate(&env);
    let failure = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let (token, token_admin_client) = create_token(&env, &token_admin);
    let amounts = [300, 0, 200];
    let total: i128 = amounts.iter().sum();
    token_admin_client.mint(&creator, &total);

    let contract_id = env.register_contract(None, AccountabilityVault);
    let contract = AccountabilityVaultClient::new(&env, &contract_id);

    let mut milestones = vec![&env];
    let due_dates = [100, 200, 300];
    for (i, due) in due_dates.iter().enumerate() {
        milestones.push_back(Milestone {
            title: String::from_str(&env, "m"),
            amount: amounts[i],
            due_date: 1_000 + due,
            verified: false,
            released: false,
        });
    }

    let end = 1_300;
    let verifier_set = VerifierSet {
        verifiers: vec![&env, verifier.clone()],
        threshold: 1u32,
    };
    let vault_id = String::from_str(&env, "v1");

    let result = contract.try_create_vault(
        &vault_id,
        &creator,
        &verifier_set,
        &None,
        &token,
        &total,
        &success,
        &failure,
        &end,
        &milestones,
        &guardian,
    );

    assert!(matches!(result, Err(Ok(Error::InvalidAmount))));
}

// ── issue #363: oracle-driven check_in path ──────────────────────────────────

#[test]
fn test_oracle_check_in_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let oracle = Address::generate(&env);
    let s = setup_with_oracle(&[100, 200], &[400, 600], Some(oracle.clone()));
    s.contract.stake(&s.vault_id, &s.creator);

    // Oracle confirms both milestones.
    s.contract.check_in(&s.vault_id, &oracle, &0, &evidence_hash(&s.env, 1));
    s.contract.check_in(&s.vault_id, &oracle, &1, &evidence_hash(&s.env, 1));

    s.contract.claim(&s.vault_id, &s.creator);
    let vault = s.contract.get_vault(&s.vault_id);
    assert_eq!(vault.status, VaultStatus::Completed);

    let token_client = token::Client::new(&s.env, &s.token);
    assert_eq!(token_client.balance(&s.success), 1_000);
}

#[test]
fn test_verifier_check_in_still_works_with_oracle_configured() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let oracle = Address::generate(&env);
    let s = setup_with_oracle(&[100], &[500], Some(oracle.clone()));
    s.contract.stake(&s.vault_id, &s.creator);

    // The human verifier can still check in even when an oracle is set.
    s.contract.check_in(&s.vault_id, &s.verifier, &0, &evidence_hash(&s.env, 1));

    let vault = s.contract.get_vault(&s.vault_id);
    assert!(vault.milestones.get(0).unwrap().verified);
}

#[test]
#[should_panic]
fn test_unauthorized_caller_check_in_fails() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);

    let random = Address::generate(&s.env);
    // Neither verifier nor oracle — must fail with Unauthorized.
    s.contract.check_in(&s.vault_id, &random, &0, &evidence_hash(&s.env, 1));
}

#[test]
#[should_panic]
fn test_oracle_not_set_random_caller_check_in_fails() {
    // No oracle configured; only the verifier is authorized.
    let s = setup_with_oracle(&[100], &[500], None);
    s.contract.stake(&s.vault_id, &s.creator);

    let fake_oracle = Address::generate(&s.env);
    s.contract.check_in(&s.vault_id, &fake_oracle, &0, &evidence_hash(&s.env, 1));
}

#[test]
fn test_vault_has_oracle_field_when_set() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let creator = Address::generate(&env);
    let verifier = Address::generate(&env);
    let oracle = Address::generate(&env);
    let guardian = Address::generate(&env);
    let success = Address::generate(&env);
    let failure = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let (token, token_admin_client) = create_token(&env, &token_admin);
    token_admin_client.mint(&creator, &500);

    let contract_id = env.register_contract(None, AccountabilityVault);
    let contract = AccountabilityVaultClient::new(&env, &contract_id);

    let verifier_set = VerifierSet {
        verifiers: vec![&env, verifier.clone()],
        threshold: 1u32,
    };
    let milestones = vec![
        &env,
        Milestone {
            title: String::from_str(&env, "goal"),
            amount: 500,
            due_date: 1_200,
            verified: false,
            released: false,
        },
    ];
    let vault_id = String::from_str(&env, "v1");
    contract.create_vault(
        &vault_id,
        &creator,
        &verifier_set,
        &Some(oracle.clone()),
        &token,
        &500,
        &success,
        &failure,
        &1_200,
        &milestones,
        &guardian,
    );

    let vault = contract.get_vault(&vault_id);
    assert_eq!(vault.oracle, Some(oracle));
}

#[test]
fn test_vault_oracle_field_is_none_when_not_set() {
    let s = setup(&[100], &[500]);
    let vault = s.contract.get_vault(&s.vault_id);
    assert_eq!(vault.oracle, None);
}

// ── cross-feature: stake_from then oracle check_in then claim ────────────────

#[test]
fn test_stake_from_oracle_checkin_claim_full_flow() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let creator = Address::generate(&env);
    let verifier = Address::generate(&env);
    let oracle = Address::generate(&env);
    let guardian = Address::generate(&env);
    let spender = Address::generate(&env);
    let success = Address::generate(&env);
    let failure = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let (token, token_admin_client) = create_token(&env, &token_admin);
    token_admin_client.mint(&creator, &500);

    let contract_id = env.register_contract(None, AccountabilityVault);
    let contract = AccountabilityVaultClient::new(&env, &contract_id);

    let verifier_set = VerifierSet {
        verifiers: vec![&env, verifier.clone()],
        threshold: 1u32,
    };
    let milestones = vec![
        &env,
        Milestone {
            title: String::from_str(&env, "goal"),
            amount: 500,
            due_date: 1_200,
            verified: false,
            released: false,
        },
    ];
    let vault_id = String::from_str(&env, "v1");
    contract.create_vault(
        &vault_id,
        &creator,
        &verifier_set,
        &Some(oracle.clone()),
        &token,
        &500,
        &success,
        &failure,
        &1_200,
        &milestones,
        &guardian,
    );

    let token_client = token::Client::new(&env, &token);
    token_client.approve(&creator, &spender, &500, &200);

    // Backend drives staking via allowance.
    contract.stake_from(&vault_id, &creator, &spender);
    assert_eq!(contract.get_vault(&vault_id).status, VaultStatus::Active);

    // Oracle confirms the milestone.
    contract.check_in(&vault_id, &oracle, &0, &evidence_hash(&env, 1));
    assert!(contract.get_vault(&vault_id).milestones.get(0).unwrap().verified);

    // Claim releases funds.
    contract.claim(&vault_id, &creator);
    assert_eq!(contract.get_vault(&vault_id).status, VaultStatus::Completed);
    assert_eq!(token_client.balance(&success), 500);
}

// ── issue #352: checks-effects-interactions ordering tests ───────────────────

#[test]
fn test_cei_slash_on_miss_state_is_terminal_before_transfer() {
    // After slash_on_miss the vault must be in Failed terminal state with
    // staked == 0 (CEI: state persisted before the external token transfer).
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.creator);

    s.env.ledger().set_timestamp(2_000);
    s.contract.slash_on_miss();

    let vault = s.contract.get_vault();
    assert_eq!(vault.status, VaultStatus::Failed);
    assert_eq!(vault.staked, 0);

    let token_client = token::Client::new(&s.env, &s.token);
    assert_eq!(token_client.balance(&s.failure), 500);
}

#[test]
fn test_cei_claim_state_is_terminal_before_transfer() {
    // After claim the vault must be in Completed terminal state with staked == 0
    // (CEI: state persisted before the external token transfer).
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.creator);
    s.contract.check_in(&s.verifier, &0, &evidence_hash(&s.env, 1));
    s.contract.claim(&s.creator);

    let vault = s.contract.get_vault();
    assert_eq!(vault.status, VaultStatus::Completed);
    assert_eq!(vault.staked, 0);

    let token_client = token::Client::new(&s.env, &s.token);
    assert_eq!(token_client.balance(&s.success), 500);
}

#[test]
fn test_cei_slash_cannot_be_triggered_twice() {
    // After a successful slash_on_miss the vault is Failed; a second call must
    // fail with NotActive — the CEI state update prevents double-slash.
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.creator);

    s.env.ledger().set_timestamp(2_000);
    s.contract.slash_on_miss();

    let result = s.contract.try_slash_on_miss();
    assert!(result.is_err());
}

#[test]
fn test_cei_claim_cannot_be_triggered_twice() {
    // After a successful claim the vault is Completed; a second call must fail
    // with NotActive — the CEI state update prevents double-claim.
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.creator);
    s.contract.check_in(&s.verifier, &0, &evidence_hash(&s.env, 1));
    s.contract.claim(&s.creator);

    let result = s.contract.try_claim(&s.creator);
    assert!(result.is_err());
}

// ── issue #357: emergency pause / unpause tests ──────────────────────────────

#[test]
#[should_panic]
fn test_pause_blocks_slash_on_miss() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.creator);
    s.contract.emergency_pause(&s.guardian);

    s.env.ledger().set_timestamp(2_000);
    // Must fail with Paused.
    s.contract.slash_on_miss();
}

#[test]
#[should_panic]
fn test_pause_blocks_claim() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.creator);
    s.contract.check_in(&s.verifier, &0, &evidence_hash(&s.env, 1));
    s.contract.emergency_pause(&s.guardian);

    // Must fail with Paused.
    s.contract.claim(&s.creator);
}

#[test]
#[should_panic]
fn test_pause_blocks_withdraw_active() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.creator);
    s.contract.emergency_pause(&s.guardian);

    // Must fail with Paused.
    s.contract.withdraw(&s.vault_id, &s.creator);
}

#[test]
fn test_unpause_allows_slash_on_miss() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.creator);
    s.contract.emergency_pause(&s.guardian);
    s.contract.emergency_unpause(&s.guardian);

    s.env.ledger().set_timestamp(2_000);
    s.contract.slash_on_miss();

    let vault = s.contract.get_vault();
    assert_eq!(vault.status, VaultStatus::Failed);
}

#[test]
fn test_unpause_allows_claim() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.creator);
    s.contract.check_in(&s.verifier, &0, &evidence_hash(&s.env, 1));
    s.contract.emergency_pause(&s.guardian);
    s.contract.emergency_unpause(&s.guardian);

    s.contract.claim(&s.creator);

    let vault = s.contract.get_vault();
    assert_eq!(vault.status, VaultStatus::Completed);
}

#[test]
#[should_panic]
fn test_non_guardian_cannot_pause() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.creator);

    let impostor = Address::generate(&s.env);
    // impostor is not the vault guardian — must fail with Unauthorized.
    s.contract.emergency_pause(&impostor);
}

#[test]
fn test_pause_does_not_block_draft_withdraw() {
    // Cancelling a Draft vault does not transfer tokens; the pause only
    // blocks the active-vault settlement paths.
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let creator = Address::generate(&env);
    let verifier = Address::generate(&env);
    let guardian = Address::generate(&env);
    let success = Address::generate(&env);
    let failure = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let (token, token_admin_client) = create_token(&env, &token_admin);
    token_admin_client.mint(&creator, &500);

    let contract_id = env.register_contract(None, AccountabilityVault);
    let contract = AccountabilityVaultClient::new(&env, &contract_id);

    let verifier_set = VerifierSet {
        verifiers: vec![&env, verifier.clone()],
        threshold: 1u32,
    };
    let milestones = vec![
        &env,
        Milestone {
            title: String::from_str(&env, "m"),
            amount: 500,
            due_date: 1_200,
            verified: false,
        },
    ];
    contract.create_vault(
        &creator, &verifier_set, &None, &token, &500, &success, &failure, &1_200,
        &milestones, &guardian,
    );

    // Pause before staking (vault is still Draft).
    contract.emergency_pause(&guardian);

    // Draft-path cancel must still succeed.
    contract.cancel_vault(&vault_id, &creator);
    let vault = contract.get_vault(&vault_id);
    assert_eq!(vault.status, VaultStatus::Cancelled);
}

// ── issue #364: M-of-N multi-verifier check_in tests ─────────────────────────

#[test]
fn test_multi_verifier_single_approval_insufficient_for_threshold_two() {
    // With 2 verifiers and threshold=2, a single approval does not verify the milestone.
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let creator = Address::generate(&env);
    let verifier1 = Address::generate(&env);
    let verifier2 = Address::generate(&env);
    let guardian = Address::generate(&env);
    let success = Address::generate(&env);
    let failure = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let (token, token_admin_client) = create_token(&env, &token_admin);
    token_admin_client.mint(&creator, &500);

    let contract_id = env.register_contract(None, AccountabilityVault);
    let contract = AccountabilityVaultClient::new(&env, &contract_id);

    let verifier_set = VerifierSet {
        verifiers: vec![&env, verifier1.clone(), verifier2.clone()],
        threshold: 2u32,
    };
    let milestones = vec![
        &env,
        Milestone {
            title: String::from_str(&env, "m"),
            amount: 500,
            due_date: 1_200,
            verified: false,
        },
    ];
    contract.create_vault(
        &creator, &verifier_set, &None, &token, &500, &success, &failure, &1_200,
        &milestones, &guardian,
    );
    contract.stake(&creator);

    // Only verifier1 approves — threshold not yet reached.
    contract.check_in(&verifier1, &0, &evidence_hash(&env, 1));
    let vault = contract.get_vault();
    assert!(!vault.milestones.get(0).unwrap().verified);
}

#[test]
fn test_multi_verifier_both_approve_verifies_milestone() {
    // With 2 verifiers and threshold=2, both approving flips the milestone to verified.
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let creator = Address::generate(&env);
    let verifier1 = Address::generate(&env);
    let verifier2 = Address::generate(&env);
    let guardian = Address::generate(&env);
    let success = Address::generate(&env);
    let failure = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let (token, token_admin_client) = create_token(&env, &token_admin);
    token_admin_client.mint(&creator, &500);

    let contract_id = env.register_contract(None, AccountabilityVault);
    let contract = AccountabilityVaultClient::new(&env, &contract_id);

    let verifier_set = VerifierSet {
        verifiers: vec![&env, verifier1.clone(), verifier2.clone()],
        threshold: 2u32,
    };
    let milestones = vec![
        &env,
        Milestone {
            title: String::from_str(&env, "m"),
            amount: 500,
            due_date: 1_200,
            verified: false,
        },
    ];
    contract.create_vault(
        &creator, &verifier_set, &None, &token, &500, &success, &failure, &1_200,
        &milestones, &guardian,
    );
    contract.stake(&creator);

    // Both verifiers approve — threshold reached.
    contract.check_in(&verifier1, &0, &evidence_hash(&env, 1));
    contract.check_in(&verifier2, &0, &evidence_hash(&env, 1));

    let vault = contract.get_vault();
    assert!(vault.milestones.get(0).unwrap().verified);
}

#[test]
#[should_panic]
fn test_multi_verifier_double_approval_by_same_verifier_fails() {
    // The same verifier may not approve the same milestone twice.
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let creator = Address::generate(&env);
    let verifier1 = Address::generate(&env);
    let verifier2 = Address::generate(&env);
    let guardian = Address::generate(&env);
    let success = Address::generate(&env);
    let failure = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let (token, token_admin_client) = create_token(&env, &token_admin);
    token_admin_client.mint(&creator, &500);

    let contract_id = env.register_contract(None, AccountabilityVault);
    let contract = AccountabilityVaultClient::new(&env, &contract_id);

    let verifier_set = VerifierSet {
        verifiers: vec![&env, verifier1.clone(), verifier2.clone()],
        threshold: 2u32,
    };
    let milestones = vec![
        &env,
        Milestone {
            title: String::from_str(&env, "m"),
            amount: 500,
            due_date: 1_200,
            verified: false,
        },
    ];
    contract.create_vault(
        &creator, &verifier_set, &None, &token, &500, &success, &failure, &1_200,
        &milestones, &guardian,
    );
    contract.stake(&creator);

    contract.check_in(&verifier1, &0, &evidence_hash(&env, 1));
    // Same verifier approves again — must fail with AlreadyApproved.
    contract.check_in(&verifier1, &0, &evidence_hash(&env, 1));
}

#[test]
fn test_multi_verifier_threshold_one_of_two_single_approval_sufficient() {
    // With 2 verifiers and threshold=1, a single approval verifies the milestone.
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let creator = Address::generate(&env);
    let verifier1 = Address::generate(&env);
    let verifier2 = Address::generate(&env);
    let guardian = Address::generate(&env);
    let success = Address::generate(&env);
    let failure = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let (token, token_admin_client) = create_token(&env, &token_admin);
    token_admin_client.mint(&creator, &500);

    let contract_id = env.register_contract(None, AccountabilityVault);
    let contract = AccountabilityVaultClient::new(&env, &contract_id);

    let verifier_set = VerifierSet {
        verifiers: vec![&env, verifier1.clone(), verifier2.clone()],
        threshold: 1u32,
    };
    let milestones = vec![
        &env,
        Milestone {
            title: String::from_str(&env, "m"),
            amount: 500,
            due_date: 1_200,
            verified: false,
        },
    ];
    contract.create_vault(
        &creator, &verifier_set, &None, &token, &500, &success, &failure, &1_200,
        &milestones, &guardian,
    );
    contract.stake(&creator);

    // Only verifier1 approves — sufficient for threshold=1.
    contract.check_in(&verifier1, &0, &evidence_hash(&env, 1));
    let vault = contract.get_vault();
    assert!(vault.milestones.get(0).unwrap().verified);
}

#[test]
fn test_multi_verifier_2of2_full_claim_flow() {
    // Two verifiers, threshold=2: both must approve each milestone before claim.
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let creator = Address::generate(&env);
    let verifier1 = Address::generate(&env);
    let verifier2 = Address::generate(&env);
    let guardian = Address::generate(&env);
    let success = Address::generate(&env);
    let failure = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let (token, token_admin_client) = create_token(&env, &token_admin);
    token_admin_client.mint(&creator, &1_000);

    let contract_id = env.register_contract(None, AccountabilityVault);
    let contract = AccountabilityVaultClient::new(&env, &contract_id);

    let verifier_set = VerifierSet {
        verifiers: vec![&env, verifier1.clone(), verifier2.clone()],
        threshold: 2u32,
    };
    let milestones = vec![
        &env,
        Milestone {
            title: String::from_str(&env, "m1"),
            amount: 400,
            due_date: 1_100,
            verified: false,
        },
        Milestone {
            title: String::from_str(&env, "m2"),
            amount: 600,
            due_date: 1_200,
            verified: false,
        },
    ];
    contract.create_vault(
        &creator, &verifier_set, &None, &token, &1_000, &success, &failure, &1_200,
        &milestones, &guardian,
    );
    contract.stake(&creator);

    // Milestone 0: both verifiers must approve.
    contract.check_in(&verifier1, &0, &evidence_hash(&env, 1));
    assert!(!contract.get_vault().milestones.get(0).unwrap().verified);
    contract.check_in(&verifier2, &0, &evidence_hash(&env, 1));
    assert!(contract.get_vault().milestones.get(0).unwrap().verified);

    // Milestone 1: both verifiers must approve.
    contract.check_in(&verifier1, &1, &evidence_hash(&env, 1));
    contract.check_in(&verifier2, &1, &evidence_hash(&env, 1));
    assert!(contract.get_vault().milestones.get(1).unwrap().verified);

    // All milestones verified — claim succeeds.
    contract.claim(&creator);
    assert_eq!(contract.get_vault().status, VaultStatus::Completed);

    let token_client = token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&success), 1_000);
}

// ── issue #XXX: withdraw refund balance preservation ─────────────────────────

/// Verifies the full fund-then-withdraw round-trip:
/// 1. Mint `amount` tokens to creator.
/// 2. Stake them into an Active vault (creator balance → 0, contract balance → amount).
/// 3. Withdraw (no check-ins, so no verified milestones) → creator balance restored,
///    contract balance zeroed, vault status Cancelled.
#[test]
fn test_withdraw_active_refunds_creator() {
    let s = setup(&[100], &[500]);
    let token_client = token::Client::new(&s.env, &s.token);
    let contract_addr = s.contract.address.clone();

    // Pre-stake: creator holds the full mint; contract holds nothing.
    assert_eq!(token_client.balance(&s.creator), 500);
    assert_eq!(token_client.balance(&contract_addr), 0);

    s.contract.stake(&s.vault_id, &s.creator);

    // Post-stake: tokens moved to contract.
    assert_eq!(token_client.balance(&s.creator), 0);
    assert_eq!(token_client.balance(&contract_addr), 500);

    // No check-ins → withdraw refunds creator and cancels the vault.
    s.contract.withdraw(&s.vault_id, &s.creator);

    let vault = s.contract.get_vault(&s.vault_id);
    assert_eq!(vault.status, VaultStatus::Cancelled);
    assert_eq!(vault.staked, 0);

    // Creator balance fully restored; contract balance is zero.
    assert_eq!(token_client.balance(&s.creator), 500);
    assert_eq!(token_client.balance(&contract_addr), 0);
}

/// Withdraw on an Active, paused vault must fail with Paused.
/// (Complements test_pause_blocks_withdraw_active with explicit vault_id pattern.)
#[test]
#[should_panic]
fn test_withdraw_active_paused_blocked() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);
    s.contract.emergency_pause(&s.guardian);
    // Must fail with Error::Paused.
    s.contract.withdraw(&s.vault_id, &s.creator);
}

// ── gas benchmarks ───────────────────────────────────────────────────────────

#[test]
fn test_gas_benchmarks_10_milestones() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let creator = Address::generate(&env);
    let verifier = Address::generate(&env);
    let guardian = Address::generate(&env);
    let success = Address::generate(&env);
    let failure = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let (token, token_admin_client) = create_token(&env, &token_admin);

    let milestone_count = 10;
    let milestone_amount = 100i128;
    let total_amount = milestone_amount * (milestone_count as i128);
    token_admin_client.mint(&creator, &total_amount);

    let contract_id = env.register_contract(None, AccountabilityVault);
    let contract = AccountabilityVaultClient::new(&env, &contract_id);

    let mut milestones = vec![&env];
    for i in 0..milestone_count {
        milestones.push_back(Milestone {
            title: String::from_str(&env, "milestone"),
            amount: milestone_amount,
            due_date: 1_000 + (i as u64 + 1) * 100,
            verified: false,
        });
    }

    let end_timestamp = 1_000 + (milestone_count as u64) * 100;
    let verifier_set = VerifierSet {
        verifiers: vec![&env, verifier.clone()],
        threshold: 1u32,
    };

    // 1. Measure create_vault
    env.budget().reset_default();
    contract.create_vault(
        &vault_id,
        &creator,
        &verifier_set,
        &None,
        &token,
        &total_amount,
        &success,
        &failure,
        &end_timestamp,
        &milestones,
        &guardian,
    );
    let create_cpu = env.budget().cpu_instruction_cost();
    let create_mem = env.budget().memory_bytes_cost();

    // 2. Measure stake
    env.budget().reset_default();
    contract.stake(&vault_id, &creator);
    let stake_cpu = env.budget().cpu_instruction_cost();
    let stake_mem = env.budget().memory_bytes_cost();

    // 3. Measure check_in
    env.budget().reset_default();
    contract.check_in(&vault_id, &verifier, &0, &evidence_hash(&env, 1));
    let check_in_cpu = env.budget().cpu_instruction_cost();
    let check_in_mem = env.budget().memory_bytes_cost();

    // Verify all remaining milestones so we can claim
    for i in 1..milestone_count {
        contract.check_in(&vault_id, &verifier, &i, &evidence_hash(&env, 1));
    }

    // 4. Measure claim
    env.budget().reset_default();
    contract.claim(&vault_id, &creator);
    let claim_cpu = env.budget().cpu_instruction_cost();
    let claim_mem = env.budget().memory_bytes_cost();

    std::println!("=== Gas Benchmarks (10 Milestones) ===");
    std::println!("create_vault: CPU = {}, Memory = {}", create_cpu, create_mem);
    std::println!("stake:        CPU = {}, Memory = {}", stake_cpu, stake_mem);
    std::println!("check_in:     CPU = {}, Memory = {}", check_in_cpu, check_in_mem);
    std::println!("claim:        CPU = {}, Memory = {}", claim_cpu, claim_mem);

    assert!(create_cpu < 600_000);
    assert!(create_mem < 200_000);

    assert!(stake_cpu < 700_000);
    assert!(stake_mem < 200_000);

    assert!(check_in_cpu < 300_000);
    assert!(check_in_mem < 100_000);

    assert!(claim_cpu < 900_000);
    assert!(claim_mem < 250_000);
}

#[test]
fn test_gas_benchmarks_slash_on_miss_10_milestones() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let creator = Address::generate(&env);
    let verifier = Address::generate(&env);
    let guardian = Address::generate(&env);
    let success = Address::generate(&env);
    let failure = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let (token, token_admin_client) = create_token(&env, &token_admin);

    let milestone_count = 10;
    let milestone_amount = 100i128;
    let total_amount = milestone_amount * (milestone_count as i128);
    token_admin_client.mint(&creator, &total_amount);

    let contract_id = env.register_contract(None, AccountabilityVault);
    let contract = AccountabilityVaultClient::new(&env, &contract_id);

    let mut milestones = vec![&env];
    for i in 0..milestone_count {
        milestones.push_back(Milestone {
            title: String::from_str(&env, "milestone"),
            amount: milestone_amount,
            due_date: 1_000 + (i as u64 + 1) * 100,
            verified: false,
        });
    }

    let end_timestamp = 1_000 + (milestone_count as u64) * 100;
    let verifier_set = VerifierSet {
        verifiers: vec![&env, verifier.clone()],
        threshold: 1u32,
    };

    contract.create_vault(
        &vault_id,
        &creator,
        &verifier_set,
        &None,
        &token,
        &total_amount,
        &success,
        &failure,
        &end_timestamp,
        &milestones,
        &guardian,
    );

    contract.stake(&creator);

    // Advance past the overall deadline to allow slash
    env.ledger().set_timestamp(end_timestamp + 1);

    // Measure slash_on_miss
    env.budget().reset_default();
    contract.slash_on_miss(&vault_id);
    let slash_cpu = env.budget().cpu_instruction_cost();
    let slash_mem = env.budget().memory_bytes_cost();

    std::println!("=== Gas Benchmarks Slash (10 Milestones) ===");
    std::println!("slash_on_miss: CPU = {}, Memory = {}", slash_cpu, slash_mem);

    assert!(slash_cpu < 900_000);
    assert!(slash_mem < 250_000);
}

// ── issue #488: symbol-typed event topics ────────────────────────────────────

/// Extracts the first XDR-decoded topic from an emitted event tuple.
/// Soroban testutils return events as `(contract_id, topics_vec, data)`.
/// The topics vector is a `Vec<Val>` — we pull index 0 and try to read it as Symbol.
fn first_topic_as_symbol(env: &Env, event_index: usize) -> Option<Symbol> {
    use soroban_sdk::IntoVal;
    let events = env.events().all();
    let (_, topics, _) = events.get(event_index as u32)?;
    let raw: soroban_sdk::Val = topics.get(0)?;
    Symbol::try_from_val(env, &raw).ok()
}

#[test]
fn test_vault_created_emits_symbol_topic() {
    let s = setup(&[100], &[500]);
    // create_vault is called inside setup; it is the first (and only so far) event.
    let events = s.env.events().all();
    assert!(!events.is_empty(), "expected at least one event");
    let (_, topics, _) = events.get(0).unwrap();
    let topic0: soroban_sdk::Val = topics.get(0).unwrap();
    let expected = Symbol::new(&s.env, "vault_created");
    let actual = Symbol::try_from_val(&s.env, &topic0).expect("topic[0] must be a Symbol");
    assert_eq!(actual, expected);
}

#[test]
fn test_vault_staked_emits_symbol_topic() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);

    // Events: [vault_created(0), vault_staked(1)]
    let events = s.env.events().all();
    assert!(events.len() >= 2, "expected at least vault_created + vault_staked");
    let (_, topics, _) = events.get(1).unwrap();
    let topic0: soroban_sdk::Val = topics.get(0).unwrap();
    let actual = Symbol::try_from_val(&s.env, &topic0).expect("topic[0] must be a Symbol");
    assert_eq!(actual, Symbol::new(&s.env, "vault_staked"));
}

#[test]
fn test_milestone_checked_in_emits_symbol_topic() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);
    s.contract.check_in(&s.vault_id, &s.verifier, &0);

    // Events: [vault_created(0), vault_staked(1), milestone_checked_in(2)]
    let events = s.env.events().all();
    assert!(events.len() >= 3);
    let (_, topics, _) = events.get(2).unwrap();
    let topic0: soroban_sdk::Val = topics.get(0).unwrap();
    let actual = Symbol::try_from_val(&s.env, &topic0).expect("topic[0] must be a Symbol");
    assert_eq!(actual, Symbol::new(&s.env, "milestone_checked_in"));
}

#[test]
fn test_milestone_checked_in_source_topic_is_symbol() {
    // The third topic (source) must be a Symbol "verifier" or "oracle".
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);
    s.contract.check_in(&s.vault_id, &s.verifier, &0);

    let events = s.env.events().all();
    assert!(events.len() >= 3);
    let (_, topics, _) = events.get(2).unwrap();
    // topics: [milestone_checked_in, caller, source]
    let source_val: soroban_sdk::Val = topics.get(2).unwrap();
    let source = Symbol::try_from_val(&s.env, &source_val).expect("source topic must be a Symbol");
    // Verifier-driven check_in must carry source = "verifier"
    assert_eq!(source, soroban_sdk::symbol_short!("verifier"));
}

#[test]
fn test_oracle_check_in_source_topic_is_oracle_symbol() {
    // We need the oracle address generated in the same env as the setup.
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);

    let creator = Address::generate(&env);
    let verifier = Address::generate(&env);
    let oracle = Address::generate(&env);
    let guardian = Address::generate(&env);
    let success = Address::generate(&env);
    let failure = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let (token, token_admin_client) = create_token(&env, &token_admin);
    token_admin_client.mint(&creator, &500);

    let contract_id = env.register_contract(None, AccountabilityVault);
    let contract = AccountabilityVaultClient::new(&env, &contract_id);

    let verifier_set = VerifierSet {
        verifiers: vec![&env, verifier.clone()],
        threshold: 1u32,
    };
    let milestones = vec![
        &env,
        Milestone {
            title: String::from_str(&env, "m"),
            amount: 500,
            due_date: 1_200,
            verified: false,
            released: false,
        },
    ];
    let vault_id = String::from_str(&env, "v1");
    contract.create_vault(
        &vault_id,
        &creator,
        &verifier_set,
        &Some(oracle.clone()),
        &token,
        &500,
        &success,
        &failure,
        &1_200,
        &milestones,
        &guardian,
    );
    contract.stake(&vault_id, &creator);
    contract.check_in(&vault_id, &oracle, &0);

    // Events: [vault_created(0), vault_staked(1), milestone_checked_in(2)]
    let events = env.events().all();
    assert!(events.len() >= 3);
    let (_, topics, _) = events.get(2).unwrap();
    // topics: [milestone_checked_in, caller, source]
    let source_val: soroban_sdk::Val = topics.get(2).unwrap();
    let source = Symbol::try_from_val(&env, &source_val).expect("source topic must be a Symbol");
    assert_eq!(source, soroban_sdk::symbol_short!("oracle"));
}

#[test]
fn test_vault_slashed_emits_symbol_topic() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);
    s.env.ledger().set_timestamp(2_000);
    s.contract.slash_on_miss(&s.vault_id);

    // Events: [vault_created(0), vault_staked(1), vault_slashed(2)]
    let events = s.env.events().all();
    assert!(events.len() >= 3);
    let (_, topics, _) = events.get(2).unwrap();
    let topic0: soroban_sdk::Val = topics.get(0).unwrap();
    let actual = Symbol::try_from_val(&s.env, &topic0).expect("topic[0] must be a Symbol");
    assert_eq!(actual, Symbol::new(&s.env, "vault_slashed"));
}

#[test]
fn test_vault_completed_emits_symbol_topic() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);
    s.contract.check_in(&s.vault_id, &s.verifier, &0);
    s.contract.claim(&s.vault_id, &s.creator);

    // Events: [vault_created(0), vault_staked(1), milestone_checked_in(2), vault_completed(3)]
    let events = s.env.events().all();
    assert!(events.len() >= 4);
    let (_, topics, _) = events.last().unwrap();
    let topic0: soroban_sdk::Val = topics.get(0).unwrap();
    let actual = Symbol::try_from_val(&s.env, &topic0).expect("topic[0] must be a Symbol");
    assert_eq!(actual, Symbol::new(&s.env, "vault_completed"));
}

#[test]
fn test_vault_cancelled_emits_symbol_topic() {
    let s = setup(&[100], &[500]);
    s.contract.withdraw(&s.vault_id, &s.creator);

    // Events: [vault_created(0), vault_cancelled(1)]
    let events = s.env.events().all();
    assert!(events.len() >= 2);
    let (_, topics, _) = events.get(1).unwrap();
    let topic0: soroban_sdk::Val = topics.get(0).unwrap();
    let actual = Symbol::try_from_val(&s.env, &topic0).expect("topic[0] must be a Symbol");
    assert_eq!(actual, Symbol::new(&s.env, "vault_cancelled"));
}

#[test]
fn test_vault_withdrawn_emits_symbol_topic() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);
    // Advance to deadline without any check-in, then try withdraw — but since
    // withdraw on an Active vault with no verified milestones is allowed, use that path.
    s.contract.withdraw(&s.vault_id, &s.creator);

    // Events: [vault_created(0), vault_staked(1), vault_withdrawn(2)]
    let events = s.env.events().all();
    assert!(events.len() >= 3);
    let (_, topics, _) = events.last().unwrap();
    let topic0: soroban_sdk::Val = topics.get(0).unwrap();
    let actual = Symbol::try_from_val(&s.env, &topic0).expect("topic[0] must be a Symbol");
    assert_eq!(actual, Symbol::new(&s.env, "vault_withdrawn"));
}

#[test]
fn test_deadline_extended_emits_symbol_topic() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);
    let vault = s.contract.get_vault(&s.vault_id);
    s.contract.extend_deadline(&s.vault_id, &s.creator, &(vault.end_timestamp + 500));

    // Events: [vault_created(0), vault_staked(1), deadline_extended(2)]
    let events = s.env.events().all();
    assert!(events.len() >= 3);
    let (_, topics, _) = events.last().unwrap();
    let topic0: soroban_sdk::Val = topics.get(0).unwrap();
    let actual = Symbol::try_from_val(&s.env, &topic0).expect("topic[0] must be a Symbol");
    assert_eq!(actual, Symbol::new(&s.env, "deadline_extended"));
}

#[test]
fn test_vault_paused_emits_symbol_topic() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);
    s.contract.emergency_pause(&s.vault_id, &s.guardian);

    let events = s.env.events().all();
    let (_, topics, _) = events.last().unwrap();
    let topic0: soroban_sdk::Val = topics.get(0).unwrap();
    let actual = Symbol::try_from_val(&s.env, &topic0).expect("topic[0] must be a Symbol");
    assert_eq!(actual, Symbol::new(&s.env, "vault_paused"));
}

#[test]
fn test_vault_unpaused_emits_symbol_topic() {
    let s = setup(&[100], &[500]);
    s.contract.stake(&s.vault_id, &s.creator);
    s.contract.emergency_pause(&s.vault_id, &s.guardian);
    s.contract.emergency_unpause(&s.vault_id, &s.guardian);

    let events = s.env.events().all();
    let (_, topics, _) = events.last().unwrap();
    let topic0: soroban_sdk::Val = topics.get(0).unwrap();
    let actual = Symbol::try_from_val(&s.env, &topic0).expect("topic[0] must be a Symbol");
    assert_eq!(actual, Symbol::new(&s.env, "vault_unpaused"));
}
