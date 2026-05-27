#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, token, Address, BytesN,
    Env, Vec,
};

#[cfg(test)]
mod test;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Milestone {
    pub amount: i128,
    pub verified: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Vault {
    pub token: Address,
    pub creator: Address,
    pub verifier: Address,
    pub success_destination: Address,
    pub failure_destination: Address,
    pub staked_amount: i128,
    pub deadline: u64,
    pub milestones: Vec<Milestone>,
    pub settled: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Settlement {
    pub success_amount: i128,
    pub failure_amount: i128,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Vault(BytesN<32>),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    VaultAlreadyExists = 1,
    VaultNotFound = 2,
    EmptyMilestones = 3,
    InvalidAmount = 4,
    MilestoneSumMismatch = 5,
    AmountOverflow = 6,
    AlreadySettled = 7,
    DeadlineNotPassed = 8,
    MilestoneOutOfRange = 9,
}

#[contract]
pub struct AccountabilityVault;

#[contractimpl]
impl AccountabilityVault {
    pub fn create_vault(
        env: Env,
        vault_id: BytesN<32>,
        token: Address,
        creator: Address,
        verifier: Address,
        success_destination: Address,
        failure_destination: Address,
        staked_amount: i128,
        deadline: u64,
        milestone_amounts: Vec<i128>,
    ) {
        creator.require_auth();

        if env.storage().persistent().has(&DataKey::Vault(vault_id.clone())) {
            panic_with_error!(&env, Error::VaultAlreadyExists);
        }
        if staked_amount <= 0 {
            panic_with_error!(&env, Error::InvalidAmount);
        }
        if milestone_amounts.is_empty() {
            panic_with_error!(&env, Error::EmptyMilestones);
        }

        let mut milestones = Vec::new(&env);
        let mut total = 0_i128;
        for amount in milestone_amounts.iter() {
            if amount <= 0 {
                panic_with_error!(&env, Error::InvalidAmount);
            }
            total = checked_add(&env, total, amount);
            milestones.push_back(Milestone {
                amount,
                verified: false,
            });
        }

        if total != staked_amount {
            panic_with_error!(&env, Error::MilestoneSumMismatch);
        }

        token::Client::new(&env, &token).transfer(
            &creator,
            &env.current_contract_address(),
            &staked_amount,
        );

        let vault = Vault {
            token,
            creator,
            verifier,
            success_destination,
            failure_destination,
            staked_amount,
            deadline,
            milestones,
            settled: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Vault(vault_id), &vault);
    }

    pub fn verify_milestone(env: Env, vault_id: BytesN<32>, milestone_index: u32) {
        let mut vault = read_vault(&env, &vault_id);
        vault.verifier.require_auth();

        if vault.settled {
            panic_with_error!(&env, Error::AlreadySettled);
        }

        let mut milestone = vault
            .milestones
            .get(milestone_index)
            .unwrap_or_else(|| panic_with_error!(&env, Error::MilestoneOutOfRange));
        milestone.verified = true;
        vault.milestones.set(milestone_index, milestone);
        write_vault(&env, &vault_id, &vault);
    }

    pub fn settlement(env: Env, vault_id: BytesN<32>) -> Settlement {
        let vault = read_vault(&env, &vault_id);
        settlement_for(&env, &vault)
    }

    pub fn claim(env: Env, vault_id: BytesN<32>) -> Settlement {
        settle_after_deadline(env, vault_id)
    }

    pub fn slash_on_miss(env: Env, vault_id: BytesN<32>) -> Settlement {
        settle_after_deadline(env, vault_id)
    }
}

fn settle_after_deadline(env: Env, vault_id: BytesN<32>) -> Settlement {
    let mut vault = read_vault(&env, &vault_id);

    if vault.settled {
        panic_with_error!(&env, Error::AlreadySettled);
    }
    if env.ledger().timestamp() < vault.deadline {
        panic_with_error!(&env, Error::DeadlineNotPassed);
    }

    let settlement = settlement_for(&env, &vault);
    let token_client = token::Client::new(&env, &vault.token);
    let contract = env.current_contract_address();

    if settlement.success_amount > 0 {
        token_client.transfer(
            &contract,
            &vault.success_destination,
            &settlement.success_amount,
        );
    }
    if settlement.failure_amount > 0 {
        token_client.transfer(
            &contract,
            &vault.failure_destination,
            &settlement.failure_amount,
        );
    }

    vault.settled = true;
    write_vault(&env, &vault_id, &vault);
    settlement
}

fn settlement_for(env: &Env, vault: &Vault) -> Settlement {
    let mut success_amount = 0_i128;
    let mut failure_amount = 0_i128;

    for milestone in vault.milestones.iter() {
        if milestone.amount <= 0 {
            panic_with_error!(env, Error::InvalidAmount);
        }

        if milestone.verified {
            success_amount = checked_add(env, success_amount, milestone.amount);
        } else {
            failure_amount = checked_add(env, failure_amount, milestone.amount);
        }
    }

    let settled_total = checked_add(env, success_amount, failure_amount);
    if settled_total != vault.staked_amount {
        panic_with_error!(env, Error::MilestoneSumMismatch);
    }

    Settlement {
        success_amount,
        failure_amount,
    }
}

fn read_vault(env: &Env, vault_id: &BytesN<32>) -> Vault {
    env.storage()
        .persistent()
        .get(&DataKey::Vault(vault_id.clone()))
        .unwrap_or_else(|| panic_with_error!(env, Error::VaultNotFound))
}

fn write_vault(env: &Env, vault_id: &BytesN<32>, vault: &Vault) {
    env.storage()
        .persistent()
        .set(&DataKey::Vault(vault_id.clone()), vault);
}

fn checked_add(env: &Env, left: i128, right: i128) -> i128 {
    left.checked_add(right)
        .unwrap_or_else(|| panic_with_error!(env, Error::AmountOverflow))
}
