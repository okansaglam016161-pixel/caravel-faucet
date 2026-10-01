//! Caravel Faucet local tests (tari_template_test_tooling).
//!
//! Every account here comes from the tooling's own XTR faucet, so each starts with 1 000 tTARI. The
//! faucet under test pays CLAIM (100 tTARI) per claim and is funded from a separate account.
//!
//! A claim in these tests deposits the returned bucket into the claimer's account so the
//! transaction finalises. Caravel instead turns it into a stealth output and pays the fee from it;
//! that path depends on the live network's fee and stealth machinery and is proven by a dry run.

use tari_template_lib::prelude::{Amount, ComponentAddress, NonFungibleAddress, RistrettoPublicKeyBytes, TARI_TOKEN};
use tari_template_test_tooling::byte_type::ToByteType;
use tari_template_test_tooling::crypto::RistrettoSecretKey;
use tari_template_test_tooling::transaction::builder::{named_args::NamedArg, MainIntent};
use tari_template_test_tooling::transaction::{args, Transaction, TransactionBuilder};
use tari_template_test_tooling::TemplateTest;

const CLAIM: u64 = 100_000_000; // 100 tTARI in µTARI

/// A key holder with a funded account.
struct Actor {
    account: ComponentAddress,
    proof: NonFungibleAddress,
    secret: RistrettoSecretKey,
    public: RistrettoPublicKeyBytes,
}

fn actor(test: &mut TemplateTest) -> Actor {
    let (account, proof, secret, public) = test.create_funded_account_with_keypair();
    Actor { account, proof, secret, public: public.to_byte_type() }
}

struct Setup {
    test: TemplateTest,
    faucet: ComponentAddress,
    admin: Actor,
}

/// A faucet owned by a fresh admin, paying CLAIM per claim, funded with `funding` µTARI.
fn setup(funding: u64) -> Setup {
    let mut test = TemplateTest::my_crate();
    let admin = actor(&mut test);
    let faucet: ComponentAddress = test.call_function(
        "CaravelFaucet",
        "new",
        args![admin.public, admin.account, Amount::from_u64(CLAIM)],
        vec![admin.proof.clone()],
    );
    let mut s = Setup { test, faucet, admin };
    if funding > 0 {
        let funder = actor(&mut s.test);
        deposit_ok(&mut s, &funder, funding);
    }
    s
}

fn tx(test: &TemplateTest) -> TransactionBuilder<MainIntent> {
    Transaction::builder_localnet(test.current_epoch())
}

fn deposit_tx(s: &Setup, from: &Actor, amount: u64) -> Transaction {
    tx(&s.test)
        .call_method(from.account, "withdraw", args![TARI_TOKEN, Amount::from_u64(amount)])
        .put_last_instruction_output_on_workspace("funds")
        .call_method(s.faucet, "deposit", args![Workspace("funds")])
        .build_and_seal(&from.secret)
}

fn deposit_ok(s: &mut Setup, from: &Actor, amount: u64) {
    let t = deposit_tx(s, from, amount);
    s.test.execute_expect_success(t, vec![from.proof.clone()]);
}

/// `signer` signs a claim naming `claimer_pk`; the bucket goes into `signer`'s account.
fn claim_tx(s: &Setup, signer: &Actor, claimer_pk: RistrettoPublicKeyBytes) -> Transaction {
    tx(&s.test)
        .call_method(s.faucet, "claim", args![claimer_pk])
        .put_last_instruction_output_on_workspace("claimed")
        .call_method(signer.account, "deposit", args![Workspace("claimed")])
        .build_and_seal(&signer.secret)
}

fn claim_ok(s: &mut Setup, who: &Actor) {
    let t = claim_tx(s, who, who.public);
    s.test.execute_expect_success(t, vec![who.proof.clone()]);
}

fn claim_fails(s: &mut Setup, who: &Actor) -> String {
    let t = claim_tx(s, who, who.public);
    s.test.execute_expect_failure(t, vec![who.proof.clone()]).to_string()
}

/// `who` calls a faucet method; returns the rejection text, or "" on success.
fn admin_call(s: &mut Setup, who: &Actor, method: &str, a: Vec<NamedArg>, expect_ok: bool) -> String {
    let t = tx(&s.test).call_method(s.faucet, method, a).build_and_seal(&who.secret);
    if expect_ok {
        s.test.execute_expect_success(t, vec![who.proof.clone()]);
        String::new()
    } else {
        s.test.execute_expect_failure(t, vec![who.proof.clone()]).to_string()
    }
}

fn account_balance(s: &mut Setup, who: &Actor) -> Amount {
    s.test.call_method(who.account, "balance", args![TARI_TOKEN], vec![who.proof.clone()])
}

fn faucet_balance(s: &mut Setup) -> Amount {
    let f = s.faucet;
    s.test.call_method(f, "balance", args![], vec![])
}

#[test]
fn claim_pays_the_claim_amount_to_the_claimer() {
    let mut s = setup(500_000_000);
    let user = actor(&mut s.test);
    let before = account_balance(&mut s, &user);

    claim_ok(&mut s, &user);

    assert_eq!(account_balance(&mut s, &user), before + Amount::from_u64(CLAIM));
    assert_eq!(faucet_balance(&mut s), Amount::from_u64(500_000_000 - CLAIM));
}

#[test]
fn a_second_claim_by_the_same_key_is_refused() {
    let mut s = setup(500_000_000);
    let user = actor(&mut s.test);
    claim_ok(&mut s, &user);

    let reason = claim_fails(&mut s, &user);
    assert!(reason.contains("Duplicate NFT token id"), "unexpected rejection: {reason}");
    assert_eq!(faucet_balance(&mut s), Amount::from_u64(500_000_000 - CLAIM));
}

#[test]
fn claiming_for_someone_elses_key_is_refused() {
    let mut s = setup(500_000_000);
    let victim = actor(&mut s.test);
    let thief = actor(&mut s.test);

    // The thief signs, but names the victim's key — the victim did not sign.
    let t = claim_tx(&s, &thief, victim.public);
    let reason = s.test.execute_expect_failure(t, vec![thief.proof.clone()]).to_string();
    assert!(reason.contains("signer badge"), "unexpected rejection: {reason}");

    // And the victim can still claim for themselves afterwards.
    claim_ok(&mut s, &victim);
}

#[test]
fn paused_refuses_claims_and_unpause_restores_them() {
    let mut s = setup(500_000_000);
    let user = actor(&mut s.test);
    let admin = clone_actor(&s.admin);

    admin_call(&mut s, &admin, "pause", args![], true);
    let paused: bool = { let f = s.faucet; s.test.call_method(f, "is_paused", args![], vec![]) };
    assert!(paused);
    let reason = claim_fails(&mut s, &user);
    assert!(reason.contains("Faucet is paused"), "unexpected rejection: {reason}");

    admin_call(&mut s, &admin, "unpause", args![], true);
    claim_ok(&mut s, &user);
}

#[test]
fn an_empty_faucet_refuses_claims() {
    let mut s = setup(0);
    let user = actor(&mut s.test);
    let reason = claim_fails(&mut s, &user);
    assert!(reason.contains("Faucet is empty"), "unexpected rejection: {reason}");

    // Not enough for a whole claim is still empty.
    let funder = actor(&mut s.test);
    deposit_ok(&mut s, &funder, CLAIM - 1);
    let reason = claim_fails(&mut s, &user);
    assert!(reason.contains("Faucet is empty"), "unexpected rejection: {reason}");
}

#[test]
fn anyone_can_deposit() {
    let mut s = setup(0);
    let stranger = actor(&mut s.test);
    deposit_ok(&mut s, &stranger, 250_000_000);
    assert_eq!(faucet_balance(&mut s), Amount::from_u64(250_000_000));
}

#[test]
fn non_owner_cannot_use_admin_methods() {
    let mut s = setup(500_000_000);
    let stranger = actor(&mut s.test);

    for (method, a) in [
        ("set_claim_amount", args![Amount::from_u64(1)]),
        ("pause", args![]),
        ("unpause", args![]),
        ("withdraw", args![Amount::from_u64(1)]),
    ] {
        let reason = admin_call(&mut s, &stranger, method, a, false);
        assert!(reason.contains("Access Denied") || reason.contains("access denied") || reason.contains("AccessDenied"),
            "{method}: unexpected rejection: {reason}");
    }
    assert_eq!(faucet_balance(&mut s), Amount::from_u64(500_000_000));
    let amount: Amount = { let f = s.faucet; s.test.call_method(f, "claim_amount", args![], vec![]) };
    assert_eq!(amount, Amount::from_u64(CLAIM));
}

#[test]
fn owner_can_use_admin_methods() {
    let mut s = setup(500_000_000);
    let admin = clone_actor(&s.admin);
    admin_call(&mut s, &admin, "pause", args![], true);
    admin_call(&mut s, &admin, "unpause", args![], true);
    admin_call(&mut s, &admin, "set_claim_amount", args![Amount::from_u64(CLAIM * 2)], true);
    admin_call(&mut s, &admin, "withdraw", args![Amount::from_u64(1)], true);
}

#[test]
fn set_claim_amount_changes_the_payout() {
    let mut s = setup(500_000_000);
    let admin = clone_actor(&s.admin);
    admin_call(&mut s, &admin, "set_claim_amount", args![Amount::from_u64(CLAIM * 3)], true);

    let user = actor(&mut s.test);
    let before = account_balance(&mut s, &user);
    claim_ok(&mut s, &user);
    assert_eq!(account_balance(&mut s, &user), before + Amount::from_u64(CLAIM * 3));
}

#[test]
fn withdraw_returns_funds_to_the_admin_account() {
    let mut s = setup(500_000_000);
    let admin = clone_actor(&s.admin);
    let before = account_balance(&mut s, &admin);

    admin_call(&mut s, &admin, "withdraw", args![Amount::from_u64(200_000_000)], true);

    assert_eq!(account_balance(&mut s, &admin), before + Amount::from_u64(200_000_000));
    assert_eq!(faucet_balance(&mut s), Amount::from_u64(300_000_000));

    // More than the faucet holds is refused, and moves nothing.
    let reason = admin_call(&mut s, &admin, "withdraw", args![Amount::from_u64(300_000_001)], false);
    assert!(reason.contains("Insufficient faucet balance"), "unexpected rejection: {reason}");
}

fn clone_actor(a: &Actor) -> Actor {
    Actor { account: a.account, proof: a.proof.clone(), secret: a.secret.clone(), public: a.public }
}
