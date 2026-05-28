# Disciplr Soroban Contracts

## accountability_vault

A time-locked capital vault on Stellar/Soroban. Creators lock funds; an admin
can upgrade the contract WASM in-place without migrating funds.

### Storage layout

| Key | Storage type | Value |
|---|---|---|
| `DataKey::Admin` | Instance | `Address` — the upgrade authority |
| `DataKey::Vault(creator)` | Persistent | `i128` — locked amount |

Instance storage is preserved across WASM upgrades; persistent storage is
preserved across WASM upgrades and ledger closings.

---

## Upgrade authorization model

### How it works

```
upgrade(new_wasm_hash: BytesN<32>)
  1. Read admin from instance storage
  2. admin.require_auth()          ← Soroban enforces a valid signature
  3. env.deployer().update_current_contract_wasm(new_wasm_hash)
```

`update_current_contract_wasm` swaps the executable bytecode of the running
contract while leaving all storage entries intact. Vault balances and the admin
address survive the upgrade.

### Why admin-only

The admin address is set once at `initialize` and stored in instance storage.
`require_auth` causes the transaction to fail unless the admin's Ed25519
signature (or a valid auth entry for a multisig / policy contract) is present.
No other address can satisfy this check.

### Upgrade procedure

1. Build and upload the new WASM to the Stellar network:
   ```bash
   stellar contract upload --wasm target/wasm32-unknown-unknown/release/accountability_vault.wasm
   # → prints <new_wasm_hash>
   ```
2. Invoke `upgrade` signed by the admin:
   ```bash
   stellar contract invoke \
     --id <CONTRACT_ID> \
     --source <ADMIN_SECRET_KEY> \
     -- upgrade \
     --new_wasm_hash <new_wasm_hash>
   ```
3. Verify the contract still responds correctly and vault balances are intact.

### Security considerations

- **Single admin key** — consider using a multisig or governance contract as
  the admin for production deployments.
- **Upgrade is irreversible** — once the WASM is replaced the old code is gone.
  Test the new WASM on testnet before upgrading mainnet.
- **No timelock** — upgrades take effect immediately. Add a timelock wrapper
  contract if delayed upgrades are required.
- **Admin transfer** — there is intentionally no `set_admin` function in this
  version. To transfer admin rights, upgrade to a new WASM that includes that
  function.

---

## Running tests

```bash
cd contracts/accountability_vault
cargo test
```

Expected output: 9 tests pass, 0 failures.
