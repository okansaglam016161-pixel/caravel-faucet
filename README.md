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

## Deployment

Not published yet.

## License

MIT — see [LICENSE](LICENSE).
