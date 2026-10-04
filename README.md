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

## Use it from your app

No sign-up or API key needed. Testnet only; may be paused or refilled.

- **1,000 tTARI per claim**, paid as a **private** coin to the claiming wallet.
- **One claim per wallet key, across all apps.** The faucet is shared: a key that claimed through
  Caravel or any other app cannot claim again here.
- **Zero-balance wallets can claim.** The fee (about 0.013 tTARI) comes out of the claim.
- **A refused claim costs nothing.** The dry run catches it before anything is submitted.

| | esmeralda |
|---|---|
| Template | `template_931854cae8c2fe2bad48fdc4aaaa618dd6abadc56012a0d7092d939aa3eff32e` |
| Component | `component_568f84a0cc7ccfe49116ee86d072f02b99e246a4750e680a8cbfcd2b7862f37b` |
| Vault | `vault_56c3c95ef08e843656e755100345d8fa0bf0a58853b40cd9d2894ab48a112ec6` |
| Receipt resource | `resource_5694c70e0eaa25809e593c2f4ee85862b3fa71ea45d9ad01bf18f8accf35d621` |

Refusals, as they appear in the dry run's reject reason:

| reason contains | meaning |
|---|---|
| `Duplicate NFT token id` | this key has already claimed |
| `Faucet is paused` | claims are switched off for now |
| `Faucet is empty` | less than one claim is left |
| `unknown or out of scope signer badge` | the transaction was not signed by the claiming key |

TypeScript, `@tari-project/ootle` 0.7 (type-checked against the SDK, and the status read and dry run
checked live from a fresh empty wallet):

```ts
// Caravel Faucet — status + claim, for any dapp on Ootle esmeralda.
// npm i @tari-project/ootle@^0.7 @tari-project/ootle-indexer@^0.7 @tari-project/ootle-secret-key-wallet@^0.7
import {
  Mask, Network, StealthTransferStatement, TARI_RESOURCE_ADDRESS, TransactionBuilder, WasmStealthCrypto,
  createOutput, publicKeyLiteral, resolveMaxEpoch, sealTransaction, signBalanceProof, signTransaction,
  stealthTransferInstruction,
} from '@tari-project/ootle'
import { IndexerProvider } from '@tari-project/ootle-indexer'
import type { SecretKeyWallet } from '@tari-project/ootle-secret-key-wallet'

const INDEXER = 'https://ootle-indexer-b.tari.com' // or https://ootle-indexer-a.tari.com
const FAUCET = 'component_568f84a0cc7ccfe49116ee86d072f02b99e246a4750e680a8cbfcd2b7862f37b'
const VAULT = 'vault_56c3c95ef08e843656e755100345d8fa0bf0a58853b40cd9d2894ab48a112ec6'
const RECEIPTS = 'resource_5694c70e0eaa25809e593c2f4ee85862b3fa71ea45d9ad01bf18f8accf35d621'

const hex = (b: Uint8Array) => Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('')
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))

async function read(path: string): Promise<any | null> {
  const res = await fetch(INDEXER + path)
  if (res.status === 404) return null
  if (!res.ok) throw new Error(`${path}: HTTP ${res.status}`)
  return res.json()
}

export type FaucetStatus =
  | { kind: 'claimed' }
  | { kind: 'paused'; claimAmount: bigint }
  | { kind: 'empty'; claimAmount: bigint; available: bigint }
  | { kind: 'open'; claimAmount: bigint; available: bigint }

/** Three reads, no transaction. `ownerPk` is the claiming wallet's public key (`wallet.getPublicKey()`). */
export async function faucetStatus(ownerPk: Uint8Array): Promise<FaucetStatus> {
  const [component, vault, receipt] = await Promise.all([
    read(`/substates/${FAUCET}`),
    read(`/substates/${VAULT}`),
    // One receipt NFT per key, burnt on claim but still found: it exists once the key has claimed.
    read(`/substates/nft_${RECEIPTS.slice('resource_'.length)}_uuid_${hex(ownerPk)}`),
  ])
  if (receipt) return { kind: 'claimed' }
  const state = component.substate.Component.body.state // [vault, claim_amount, paused, receipts, admin_account]
  const claimAmount = BigInt(state[1])
  if (state[2] === true) return { kind: 'paused', claimAmount }
  const available = BigInt(vault.substate.Vault.resource_container.Stealth.revealed_amount)
  return available < claimAmount ? { kind: 'empty', claimAmount, available } : { kind: 'open', claimAmount, available }
}

/**
 * Claim `claimAmount` µT into `wallet` as a private coin. Works from a zero balance: the fee is paid
 * out of the claim. Returns the transaction id once it is finalized with Accept.
 */
export async function claim(wallet: SecretKeyWallet, claimAmount: bigint): Promise<string> {
  const network = Network.Esmeralda
  const ownerPk = await wallet.getPublicKey()
  const address = await wallet.getAddress()
  const provider = await IndexerProvider.connect({ url: INDEXER, network })
  const crypto = new WasmStealthCrypto(network)
  const maxEpoch = await resolveMaxEpoch(provider, 10)

  // The statement commits to the fee, so the transaction is built once to price it and once to send.
  async function build(fee: bigint, dryRun: boolean) {
    const { statement: outputs, outputMask } = await crypto.generateOutputsStatement(
      [createOutput({ destination: address, amount: claimAmount - fee, resourceAddress: TARI_RESOURCE_ADDRESS })],
      { amount: fee, receiver: ownerPk }, // the fee slice, paid below
    )
    const inputs = await crypto.buildInputsStatement([], claimAmount) // the claim bucket is the only input
    const proof = await signBalanceProof(crypto, Mask.zero(), outputMask, inputs, outputs)
    const statement = new StealthTransferStatement(inputs, outputs, proof)
    const tx = new TransactionBuilder(network, maxEpoch)
      .withFeeInstructionsBuilder((b) => b
        .callMethod({ componentAddress: FAUCET, methodName: 'claim' }, [publicKeyLiteral(ownerPk)])
        .saveVar('payout')
        .addInstruction(stealthTransferInstruction(
          { resourceAddress: TARI_RESOURCE_ADDRESS, revealedInputBucket: 'payout', statement },
          (name) => b.resolveWorkspaceOffsetId(name),
        ))
        .saveVar('fee')
        .addInstruction({ PayFeeFromBucket: { bucket: b.resolveWorkspaceOffsetId('fee') } }))
      .withInputs([
        { substate_id: FAUCET, version: null },
        { substate_id: VAULT, version: null },
        { substate_id: RECEIPTS, version: null },
      ])
      .buildUnsignedTransaction()
    // Signed by the claiming key — claim() refuses anyone else. dry_run rides inside the envelope.
    return sealTransaction(await signTransaction([wallet], dryRun ? { ...tx, dry_run: true } : tx))
  }

  // 1. Dry run with a generous fee. A refusal (already claimed, paused, empty) shows up here, free.
  const res = await fetch(`${INDEXER}/transactions/dry-run`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ transaction: await build(50_000n, true) }),
  })
  const finalize = (await res.json())?.result?.finalize
  if (finalize?.result?.Accept === undefined) {
    throw new Error(`Claim refused: ${JSON.stringify(finalize?.result ?? `HTTP ${res.status}`)}`)
  }
  const receipt = finalize.fee_receipt
  const fee = ((BigInt(receipt.total_fees_paid) - BigInt(receipt.total_fee_overcharge)) * 125n) / 100n // +25%

  // 2. Submit for real, then wait for the verdict. Only Accept means the coin landed.
  const { transaction_id: txId } = await provider.submitTransaction(await build(fee, false))
  try {
    for (let i = 0; i < 60; i++) {
      await sleep(3000)
      const { result } = await provider.getTransactionResult(txId).catch(() => ({ result: 'Pending' as const }))
      if (result === 'Pending') continue
      if ('Rejected' in result) throw new Error(`Claim rejected: ${result.Rejected.details}`)
      const outcome = result.Finalized.execution_result?.finalize.result
      if (outcome && 'Accept' in outcome) return txId
      throw new Error(`Claim did not succeed: ${JSON.stringify(outcome ?? result.Finalized.final_decision)}`)
    }
    throw new Error(`No verdict yet for ${txId}. Check its status before claiming again.`)
  } finally {
    provider.stopWatcher()
  }
}
```

Usage:

```ts
import { Network } from '@tari-project/ootle'
import { SecretKeyWallet } from '@tari-project/ootle-secret-key-wallet'

const wallet = SecretKeyWallet.randomWithViewKey(Network.Esmeralda) // or your user's wallet
const status = await faucetStatus(await wallet.getPublicKey())
if (status.kind === 'open') console.log('claim tx', await claim(wallet, status.claimAmount))
```

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

## Used by Caravel

[Caravel](https://github.com/okansaglam016161-pixel/caravel) offers this faucet to its wallets as
"Claim test funds" — its only faucet; Tari's built-in faucet is not used.

### How Caravel claims

- **Claim → private coin.** One transaction, exactly the recipe above: the payout becomes a private
  coin in the wallet, with no account component created.
- **The fee comes from the claim.** A wallet with a zero balance can claim; it receives
  `claim_amount − fee` (about 999.987 tTARI at 1,000 per claim). The fee is priced by a dry run
  first, and a refused claim — already claimed, paused, empty — is caught there, before anything is
  submitted.
- **One per key.** The claim is signed by, and names, the wallet's own key, so each wallet claims once.
- **Status is read from the chain**, never guessed from a balance: the faucet component (claim
  amount, paused), its vault (what is left), and the wallet's receipt NFT
  (`nft_<receipts resource>_uuid_<public key>` — present once the key has claimed, even though the
  NFT is burnt). Caravel shows the claim when the faucet is open, "check back soon" when it is paused
  or empty, and nothing once the wallet has claimed.

The code is in Caravel's
[`src/crypto/faucet.ts`](https://github.com/okansaglam016161-pixel/caravel/blob/ootle-0.42/src/crypto/faucet.ts)
and [`src/crypto/faucetStatus.ts`](https://github.com/okansaglam016161-pixel/caravel/blob/ootle-0.42/src/crypto/faucetStatus.ts).

## Build & test

The template is in [`template/`](template/), built with `tari_template_lib`
0.33 and `tari_ootle_template_build` 0.13, and tested with the Ootle 0.43 test tooling
(`tari_template_test_tooling` 0.43).

```bash
cd template
cargo test                                              # local logic tests
cargo build --release --target wasm32-unknown-unknown   # the WASM to publish
```

## Live deployment — published on Ootle 0.42, running on 0.43 (re-verified live on 0.43)

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

**Funding:** 598,000 tTARI on 2026-10-04.

| date | deposit | tx |
|---|---|---|
| 2026-10-02 | 100,000 tTARI | `0fae01fec6b8d15df1b0e2898ba07efac45427d2e345fa0bf386a0ff15dbb487` (Accept, fee 2,016 µT) |
| 2026-10-04 | 500,000 tTARI | `ea94bedd825c18e2dd4e325c465d3c290e3e34513399244a8e1ca7d6ee0dcae9` (Accept, fee 2,016 µT) |

Between the two, claims took it from 100,000 to 98,000 tTARI.

## License

MIT — see [LICENSE](LICENSE).
