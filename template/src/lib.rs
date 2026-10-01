//   Caravel Faucet — a testnet TARI faucet for Ootle.
//
//   RULES
//     - A fixed amount per claim (`claim_amount`), which the admin can change.
//     - ONE claim per public key, and only the holder of that key can make it: `claim(claimer)`
//       requires a signature by `claimer` on the transaction. Nobody can claim for someone else.
//     - The claim RETURNS the bucket rather than depositing it anywhere, so the claimer's own
//       transaction decides where it goes — Caravel turns it into a private (stealth) output and
//       pays the fee from it, which is what lets a wallet with a zero balance claim.
//     - Anyone can deposit TARI. Only the admin can change the amount, pause, unpause, or
//       withdraw — and a withdrawal always lands in the admin's account, fixed at construction.
//
//   ONE CLAIM PER KEY, WITHOUT A GROWING LIST
//     Each claim mints a receipt NFT whose id is the claimer's public key, then burns it at once.
//     A burned NFT's substate key persists, so a second mint of the same id fails with
//     "Duplicate NFT token id" and the whole transaction rejects. This is the pattern Tari's own
//     builtin faucet uses; it costs the same on every claim, where a set of claimed keys in
//     component state would be re-billed in full on every write and grow without bound.

use tari_template_lib::prelude::*;

#[template]
mod caravel_faucet {
    use super::*;

    pub struct CaravelFaucet {
        /// The TARI held for claims (revealed balance of the stealth TARI resource).
        vault: Vault,
        /// What one claim pays out, in µTARI.
        claim_amount: Amount,
        /// While true, every claim is refused. Deposits and admin methods still work.
        paused: bool,
        /// The claim-receipt NFT resource. Only this component can mint or burn it.
        receipts: ResourceAddress,
        /// Where `withdraw` sends funds: the admin's own account. Fixed at construction.
        admin_account: ComponentAddress,
    }

    impl CaravelFaucet {
        /// Deploy a faucet owned by `admin_pk`.
        ///
        /// `admin_account` is where withdrawals go — the deployer's account, which is owned by
        /// `admin_pk`. It cannot be changed afterwards, so a withdrawal can only ever return funds
        /// to the deployer. The faucet starts empty and unpaused; fund it with `deposit`.
        pub fn new(
            admin_pk: RistrettoPublicKeyBytes,
            admin_account: ComponentAddress,
            claim_amount: Amount,
        ) -> Component<Self> {
            assert!(claim_amount.is_positive(), "Claim amount must be greater than zero");

            // The receipt resource's mint/burn rule has to name THIS component, whose address only
            // exists after `create()` — so the address is allocated first.
            let allocation = CallerContext::allocate_component_address(None);
            let me = allocation.get_address();

            let receipts = ResourceBuilder::non_fungible()
                .with_token_symbol("CFCLAIM")
                .mintable(rule!(component(me)), LOCKED)
                .burnable(rule!(component(me)), LOCKED)
                .build();

            // Deny by default. The owner (admin_pk) bypasses method rules, so the admin methods
            // need no rule of their own; everyone else gets only what is listed here.
            let access_rules = ComponentAccessRules::new()
                .add_method_rule("claim", rule!(allow_all))
                .add_method_rule("deposit", rule!(allow_all))
                .add_method_rule("balance", rule!(allow_all))
                .add_method_rule("claim_amount", rule!(allow_all))
                .add_method_rule("is_paused", rule!(allow_all))
                .default(rule!(deny_all));

            Component::new(Self {
                vault: Vault::new_empty(TARI_TOKEN),
                claim_amount,
                paused: false,
                receipts,
                admin_account,
            })
            .with_address_allocation(allocation)
            .with_owner_rule(OwnerRule::ByPublicKey(admin_pk))
            .with_access_rules(access_rules)
            .create()
        }

        /// Claim `claim_amount` for `claimer`. The transaction must be signed by `claimer`.
        ///
        /// Returns the TARI as a bucket for the caller's transaction to place. Call this directly
        /// as a transaction instruction: signer badges only reach the top-level call.
        pub fn claim(&self, claimer: RistrettoPublicKeyBytes) -> Bucket {
            assert!(!self.paused, "Faucet is paused: claims are temporarily disabled");

            // Proof that `claimer` signed this transaction. The engine rejects the transaction
            // ("unknown or out of scope signer badge") if they did not.
            CallerContext::get_signer_proof_for_public_key(claimer).drop();

            let available = self.vault.balance();
            assert!(
                available >= self.claim_amount,
                "Faucet is empty: {} µTARI available, {} needed for a claim",
                available,
                self.claim_amount
            );

            // One claim per key: a second mint of this id fails with "Duplicate NFT token id".
            ResourceManager::get(self.receipts)
                .mint_non_fungible(NonFungibleId::from_u256(claimer.into_array()), &(), &())
                .burn();

            self.vault.withdraw(self.claim_amount)
        }

        /// Add TARI to the faucet. Anyone may deposit.
        pub fn deposit(&self, bucket: Bucket) {
            assert!(
                bucket.resource_address() == TARI_TOKEN,
                "Only TARI can be deposited into the faucet"
            );
            self.vault.deposit(bucket);
        }

        // ── Admin (owner only) ───────────────────────────────────────────────────

        /// Change what one claim pays out, in µTARI.
        pub fn set_claim_amount(&mut self, amount: Amount) {
            assert!(amount.is_positive(), "Claim amount must be greater than zero");
            self.claim_amount = amount;
        }

        /// Refuse all claims until `unpause`.
        pub fn pause(&mut self) {
            self.paused = true;
        }

        /// Accept claims again.
        pub fn unpause(&mut self) {
            self.paused = false;
        }

        /// Send `amount` µTARI from the faucet to the admin's account.
        pub fn withdraw(&self, amount: Amount) {
            assert!(amount.is_positive(), "Withdraw amount must be greater than zero");
            let available = self.vault.balance();
            assert!(
                available >= amount,
                "Insufficient faucet balance: {} µTARI available, {} requested",
                available,
                amount
            );
            let bucket = self.vault.withdraw(amount);
            ComponentManager::get(self.admin_account).invoke("deposit", args![bucket]);
        }

        // ── Reads (anyone) ───────────────────────────────────────────────────────

        /// TARI available for claims, in µTARI.
        pub fn balance(&self) -> Amount {
            self.vault.balance()
        }

        /// What one claim pays out, in µTARI.
        pub fn claim_amount(&self) -> Amount {
            self.claim_amount
        }

        pub fn is_paused(&self) -> bool {
            self.paused
        }
    }
}
