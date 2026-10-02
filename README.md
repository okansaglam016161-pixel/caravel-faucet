# Caravel Faucet

A testnet **TARI faucet** for the [Tari Ootle](https://ootle.tari.com) network, written as an Ootle
template. It is what [Caravel](https://github.com/okansaglam016161-pixel/caravel) uses to give new
wallets test funds, and anyone can use or fund it.

> **Testnet only.** It hands out test TARI on esmeralda. It is not designed for value.

## Rules

- **A fixed amount per claim** — `claim_amount`, set by the admin (starting at 1 000 tTARI).
- **One claim per public key**, and only the holder of that key can make it: `claim(claimer)`
  requires the transaction to be signed by `claimer`. Nobody can claim on someone else's behalf.
- **Zero-balance wallets can claim.** The claim returns the TARI as a bucket instead of depositing
  it, so the claimer's transaction can pay its own fee out of that bucket and keep the rest.
- **Anyone can deposit.** Only TARI is accepted.
- **Admin** (the deployer's key) can change the amount, pause and unpause claims, and withdraw. A
  withdrawal always goes to the admin's account, fixed when the faucet is deployed.

One claim per key is enforced without keeping a list: each claim mints a receipt NFT whose id is the
claimer's public key and burns it immediately. A burned NFT id can never be minted again, so a second
claim by the same key fails with `Duplicate NFT token id`. The cost of a claim does not grow with the
number of claims. (This is the same pattern Tari's builtin faucet uses.)

Keys are free to create, so "one claim per key" limits each wallet, not each person. Keep the
faucet's balance sized for that.

## Methods

| method | who | what |
|--------|-----|------|
| `new(admin_pk, admin_account, claim_amount)` | deployer | Create an empty, unpaused faucet owned by `admin_pk`; withdrawals go to `admin_account`. |
| `claim(claimer_pk) -> Bucket` | anyone, signed by `claimer_pk` | Pay out `claim_amount`, once per key. |
| `deposit(bucket)` | anyone | Add TARI. |
| `balance()`, `claim_amount()`, `is_paused()` | anyone | Reads. |
| `set_claim_amount(amount)` | admin | Change the payout (µTARI). |
| `pause()`, `unpause()` | admin | Stop / resume claims. |
| `withdraw(amount)` | admin | Move `amount` µTARI to the admin's account. |

Refusals, as the transaction's reject reason:

| situation | message |
|-----------|---------|
| paused | `Faucet is paused: claims are temporarily disabled` |
| not enough for a claim | `Faucet is empty: … available, … needed for a claim` |
| this key already claimed | `Duplicate NFT token id: …` (from the engine) |
| claiming for a key that did not sign | `Encountered unknown or out of scope signer badge with public key …` (from the engine) |
| admin method by anyone else | access denied (from the engine) |
| deposit of anything but TARI | `Only TARI can be deposited into the faucet` |
| withdraw more than held | `Insufficient faucet balance: … available, … requested` |

## How to claim

Call `claim` directly as an instruction (signer badges only reach the top-level call), signed by the
claiming key, and place the returned bucket. A wallet with no balance pays the fee from the claim,
all inside the fee instructions so the claim, the payout and the fee commit together or not at all:

1. `CallMethod faucet.claim(your_public_key)` → bucket
2. `StealthTransfer` with that bucket as its revealed input → a stealth (private) output of
   `claim_amount − fee` to yourself, plus a revealed output of `fee` (receiver: your key)
3. `PayFeeFromBucket` with the revealed fee output

Declare the faucet component, its vault and the receipt resource as inputs. Dry-run first to price the
fee. A wallet that already holds funds can instead pay the fee normally and deposit the bucket into its
account.

## How to refill

Anyone can top it up with a transaction that withdraws TARI from their account and calls
`faucet.deposit(bucket)`.

## Build & test

The template is in [`template/`](template/), pinned to the Ootle 0.42 crate set (`tari_template_lib`
0.33, `tari_ootle_template_build` 0.13, `tari_template_test_tooling` 0.42).

```bash
cd template
cargo test                                              # local logic tests
cargo build --release --target wasm32-unknown-unknown   # the WASM to publish
```

## Live deployment — Ootle 0.42

Published on **esmeralda** on 2026-10-02 (epoch 11752).

| | |
|---|---|
| Template | `template_931854cae8c2fe2bad48fdc4aaaa618dd6abadc56012a0d7092d939aa3eff32e` |
| Transaction | `660614bcfbe81e0d461a30b5d9a209ca3d56cf6e1686bd137733c4682b7ee222` — Commit / Accept |
| Fee | 848,187 µT (0.848187 tTARI) |
| Source | commit [`8514c95`](https://github.com/okansaglam016161-pixel/caravel-faucet/commit/8514c95dec1139031d77eff9d6aace26df850f63) |
| Metadata hash | `1220dae9e1358fe151496c47320a165f49f77706f575858a4a4c59ffbf18d1b2960f` |
| On-chain binary | 123,807 bytes, sha256 `2af2fb3136202384f04cc005b5713e363eddd51fd5f07ee5baf83e2bba561fd8` |

The on-chain binary is the release WASM after two `wasm-opt` passes: one by `tari publish`, one by
the wallet daemon. It is not byte-identical to the `cargo build` output.

### The faucet

| | |
|---|---|
| Component | `component_568f84a0cc7ccfe49116ee86d072f02b99e246a4750e680a8cbfcd2b7862f37b` |
| Transaction | `d437f79d70677a090e5a7b41b94924a94e33d3d22feaa8a16cacafdb92e6d868` — Commit / Accept |
| Fee | 2,466 µT |
| Claim amount | 1,000 tTARI (1,000,000,000 µT) |
| Owner (admin key) | `20db90bffb62905d14b75369de9de8523d859bdec5f75e36929fbf9099781661` |
| Withdrawals go to | `component_4c7dddbe61ec6e77430cc4398535d29e127c1f6d46be4500ce8f2ab164e422bc` |

Created with [`scripts/instantiate.py`](scripts/instantiate.py), which dry-runs `new()` before submitting
it and requires a final Accept. It started empty and unpaused.

## License

MIT — see [LICENSE](LICENSE).
