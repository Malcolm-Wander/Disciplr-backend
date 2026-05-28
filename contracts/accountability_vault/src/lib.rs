#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env};

// ── Storage keys ─────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Admin,
    Vault(Address), // creator → locked amount (i128)
}

// ── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct AccountabilityVault;

#[contractimpl]
impl AccountabilityVault {
    // ── Initialisation ───────────────────────────────────────────────────────

    /// Must be called once after deployment. Sets the admin address.
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
    }

    // ── Admin helpers ────────────────────────────────────────────────────────

    pub fn admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    // ── Vault operations ─────────────────────────────────────────────────────

    /// Lock `amount` tokens for `creator`. Overwrites any existing balance.
    pub fn lock(env: Env, creator: Address, amount: i128) {
        creator.require_auth();
        assert!(amount > 0, "amount must be positive");
        env.storage()
            .persistent()
            .set(&DataKey::Vault(creator), &amount);
    }

    /// Return the locked amount for `creator` (0 if none).
    pub fn balance(env: Env, creator: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Vault(creator))
            .unwrap_or(0)
    }

    // ── Upgrade ──────────────────────────────────────────────────────────────

    /// Replace the contract WASM with `new_wasm_hash`.
    /// Only the stored admin may call this; vault state is preserved.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod test;
