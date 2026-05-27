# Disciplr Contracts

## accountability_vault

The `accountability_vault` contract escrows a creator's stake and settles it by
milestone outcome after the vault deadline.

Settlement is intentionally amount-based, not count-based:

```text
success_amount = sum(milestone.amount where milestone.verified)
failure_amount = sum(milestone.amount where !milestone.verified)
success_amount + failure_amount == staked_amount
```

`create_vault` rejects any milestone set whose amounts do not sum exactly to the
staked amount. Settlement recomputes and asserts the same invariant before any
transfer, so there is no rounding path or residual balance leak. Verified
milestone value transfers to `success_destination`; unverified milestone value
transfers to `failure_destination`.

Both `claim` and `slash_on_miss` use the same settlement helper after the
deadline, so mixed outcomes are handled consistently whichever flow is called.
Settlement is single-use.

## Tests

Run the contract tests from the contracts workspace in a Rust-enabled
environment:

```bash
cd contracts
cargo test
```

The mixed-outcome tests cover partial success, remainder slash, all-success,
all-failure, sum-equals-staked validation, deadline enforcement, and single-use
settlement.

