#!/usr/bin/env python3
"""Instantiate the Caravel Faucet: call CaravelFaucet::new() through the wallet daemon.

    source ~/tari-tools/deployer.env          # exports TARI_WALLET_DAEMON_API_KEY
    python3 scripts/instantiate.py            # dry run only
    python3 scripts/instantiate.py --submit   # dry run, then submit and wait for the final result

admin_pk and admin_account are read from the wallet daemon (the fee account's owner key and
component), never typed. Fees are paid from the fee account's public (revealed) balance: the
manifest's default fee instruction is `account.pay_fee(max_fee)` on the daemon's DEFAULT account,
so the script refuses to run unless the fee account is the default.

The API key is read from the environment and only ever sent as a bearer token.
"""

import argparse
import json
import os
import sys
import urllib.request

TEMPLATE = "931854cae8c2fe2bad48fdc4aaaa618dd6abadc56012a0d7092d939aa3eff32e"
CLAIM_AMOUNT = 1_000_000_000  # µT = 1,000 tTARI
NETWORK = "esmeralda"

# Dry-run fee cap. Anything above the real cost is refunded, so this only has to be enough.
DRY_RUN_MAX_FEE = 1_000_000
# Head-room over the dry run's required fee for the real submission (also refunded if unused).
FEE_MARGIN = 1.10


def rpc(url, key, method, params):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode()
    req = urllib.request.Request(
        url,
        data=body,
        headers={"Content-Type": "application/json", "Authorization": f"Bearer {key}"},
    )
    with urllib.request.urlopen(req, timeout=600) as resp:
        out = json.load(resp)
    if out.get("error"):
        sys.exit(f"{method} failed: {out['error']}")
    return out["result"]


def manifest(admin_pk, admin_account):
    return f"""
use template_{TEMPLATE} as CaravelFaucet;

fn main() {{
    CaravelFaucet::new(
        public_key!("{admin_pk}"),
        address!("{admin_account}"),
        amount!({CLAIM_AMOUNT}),
    );
}}
"""


def new_faucet_component(finalize):
    """The component created by new(): an `up` component substate from our template."""
    accept = finalize["result"].get("Accept")
    if accept is None:
        return None
    found = []
    for substate_id, substate in accept["up_substates"]:
        if not str(substate_id).startswith("component_"):
            continue
        header = substate["substate"].get("Component", {}).get("header", {})
        if header.get("template_address") == TEMPLATE:
            found.append(substate_id)
    if len(found) != 1:
        sys.exit(f"expected exactly one new faucet component, found {found}")
    return found[0]


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--wallet", default="http://localhost:5100/json_rpc")
    ap.add_argument("--account", default="deployer", help="fee account; also the faucet admin")
    ap.add_argument("--submit", action="store_true", help="submit for real after the dry run")
    args = ap.parse_args()

    key = os.environ.get("TARI_WALLET_DAEMON_API_KEY")
    if not key:
        sys.exit("TARI_WALLET_DAEMON_API_KEY is not set")

    info = rpc(args.wallet, key, "wallet.get_info", {})
    if info["network"] != NETWORK:
        sys.exit(f"wallet daemon is on {info['network']}, expected {NETWORK}")

    account = rpc(args.wallet, key, "accounts.get", {"name_or_address": args.account})["account"]
    if not account["is_default"]:
        sys.exit(f"{args.account} is not the default account, so it would not be the fee payer")
    admin_pk = account["owner_public_key"]
    admin_account = account["component_address"]
    print(f"admin_pk      {admin_pk}")
    print(f"admin_account {admin_account}")
    print(f"claim_amount  {CLAIM_AMOUNT} µT")

    text = manifest(admin_pk, admin_account)

    def submit(max_fee, dry_run):
        return rpc(args.wallet, key, "transactions.submit_manifest", {
            "manifest": text,
            "variables": {},
            "max_fee": max_fee,
            "dry_run": dry_run,
        })

    dry = submit(DRY_RUN_MAX_FEE, True)
    finalize = dry["result"]["finalize"]
    component = new_faucet_component(finalize)
    if component is None:
        sys.exit(f"dry run did not Accept: {json.dumps(finalize['result'])[:500]}")
    required = dry["required_fees"]
    print(f"dry run       Accept, required fee {required} µT, would create {component}")
    print("              (a dry run's component address is not the final one: it is derived from the transaction)")

    if not args.submit:
        return

    max_fee = int(required * FEE_MARGIN)
    sent = submit(max_fee, False)
    tx_id = sent["transaction_id"]
    print(f"submitted     {tx_id} (max fee {max_fee} µT)")

    waited = rpc(args.wallet, key, "transactions.wait_result", {"transaction_id": tx_id, "timeout_secs": 300})
    if waited["timed_out"] or waited["result"] is None:
        sys.exit(f"no final result for {tx_id} yet (status {waited['status']}); check it before retrying")
    component = new_faucet_component(waited["result"])
    if component is None:
        sys.exit(f"{tx_id} did not Accept: {json.dumps(waited['result']['result'])[:500]}")
    print(f"ACCEPT        status {waited['status']}, fee {waited['final_fee']} µT")
    print(f"faucet        {component}")
    print(f"transaction   {tx_id}")


if __name__ == "__main__":
    main()
