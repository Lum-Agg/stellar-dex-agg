#!/usr/bin/env python3
"""Suggest ARB_BRIDGE_TOKENS from third-party volume × LumAgg graph.

Pulls Stellar Index 24h volume leaders, resolves SAC ids via Stellar.Expert,
keeps only assets present in LumAgg /api/v1/tokens, then verifies both legs
of a round-trip quote against native XLM (Soroban AMMs, max_hops=2).

Usage:
  python3 scripts/suggest_bridge_tokens.py
  python3 scripts/suggest_bridge_tokens.py --bridges "$(ssh host '...' )"
  ARB_BRIDGE_TOKENS=C...,C... python3 scripts/suggest_bridge_tokens.py

Outputs a ranked table + a ready-to-paste ARB_BRIDGE_TOKENS line for ADD candidates.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from typing import Any

XLM_SAC = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"
STELLAR_INDEX = "https://api.stellarindex.io/v1/assets"
STELLAR_EXPERT = "https://api.stellar.expert/explorer/public/asset"
DEFAULT_QUOTE_API = "https://api.lumagg.xyz"


@dataclass
class RankedAsset:
    code: str
    issuer: str
    asset_id: str
    volume_24h_usd: float
    sac: str | None = None
    in_graph: bool = False
    route_ok: bool | None = None
    amount_out: str | None = None
    error: str | None = None


def http_json(url: str, timeout: float = 30.0) -> Any:
    req = urllib.request.Request(url, headers={"User-Agent": "lumagg-suggest-bridges/1.0"})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode())


def fetch_volume_leaders(limit: int) -> list[RankedAsset]:
    # Fetch a buffer then sort locally (API default order is not always volume).
    data = http_json(f"{STELLAR_INDEX}?limit={max(limit * 2, 40)}")
    rows = data.get("data", data) if isinstance(data, dict) else data
    out: list[RankedAsset] = []
    for row in rows:
        if row.get("type") and row.get("type") != "classic":
            continue
        code = row.get("code") or ""
        issuer = row.get("issuer") or ""
        if not code or not issuer:
            continue
        if code.upper() == "XLM":
            continue
        try:
            vol = float(row.get("volume_24h_usd") or 0)
        except (TypeError, ValueError):
            vol = 0.0
        out.append(
            RankedAsset(
                code=code,
                issuer=issuer,
                asset_id=row.get("asset_id") or f"{code}-{issuer}",
                volume_24h_usd=vol,
            )
        )
    out.sort(key=lambda a: a.volume_24h_usd, reverse=True)
    return out[:limit]


def resolve_sac(code: str, issuer: str) -> str | None:
    url = f"{STELLAR_EXPERT}/{urllib.parse.quote(f'{code}-{issuer}')}"
    try:
        data = http_json(url)
    except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError, json.JSONDecodeError):
        return None
    contract = data.get("contract")
    return contract if isinstance(contract, str) and contract.startswith("C") else None


def fetch_graph_tokens(quote_api: str) -> dict[str, str]:
    """SAC/id -> symbol from LumAgg token list (assets currently in the pool graph)."""
    data = http_json(f"{quote_api.rstrip('/')}/api/v1/tokens")
    tokens = data.get("tokens", [])
    return {t["id"]: t.get("symbol") or t.get("name") or t["id"] for t in tokens if t.get("id")}


def quote_ok(
    quote_api: str,
    token_in: str,
    token_out: str,
    amount_in: str,
    max_hops: int,
) -> tuple[bool, str | None, str | None]:
    q = urllib.parse.urlencode(
        {
            "token_in": token_in,
            "token_out": token_out,
            "amount_in": amount_in,
            "max_hops": max_hops,
            "max_splits": 1,
            "prefer_soroban": 1,
        }
    )
    url = f"{quote_api.rstrip('/')}/api/v1/quote?{q}"
    try:
        data = http_json(url, timeout=20.0)
    except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError, json.JSONDecodeError) as e:
        return False, None, str(e)
    if not data.get("success"):
        return False, None, data.get("error") or "quote failed"
    expected = (data.get("data") or {}).get("expected_output")
    return True, expected, None


def parse_bridges(raw: str | None) -> set[str]:
    if not raw:
        return set()
    return {p.strip() for p in raw.split(",") if p.strip()}


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--quote-api", default=os.environ.get("ARB_QUOTE_API_URL", DEFAULT_QUOTE_API))
    p.add_argument(
        "--bridges",
        default=os.environ.get("ARB_BRIDGE_TOKENS", ""),
        help="Comma-separated current ARB_BRIDGE_TOKENS (or set env)",
    )
    p.add_argument("--top", type=int, default=30, help="How many volume leaders to consider")
    p.add_argument("--min-volume", type=float, default=1000.0, help="Min 24h USD volume")
    p.add_argument("--amount-in", default="100000000", help="Probe amount in stroops (default 10 XLM)")
    p.add_argument("--max-hops", type=int, default=2)
    p.add_argument("--sleep", type=float, default=0.15, help="Pause between Expert/quote calls")
    p.add_argument("--skip-quote", action="store_true", help="Only check graph membership, skip quotes")
    args = p.parse_args()

    current = parse_bridges(args.bridges)
    if not current:
        print(
            "warning: no --bridges / ARB_BRIDGE_TOKENS; treating all route-ok as ADD",
            file=sys.stderr,
        )

    print(f"quote_api={args.quote_api} top={args.top} min_volume={args.min_volume}", file=sys.stderr)
    print("fetching Stellar Index volume leaders…", file=sys.stderr)
    leaders = fetch_volume_leaders(args.top)
    leaders = [a for a in leaders if a.volume_24h_usd >= args.min_volume]

    print("fetching LumAgg token graph…", file=sys.stderr)
    graph = fetch_graph_tokens(args.quote_api)
    graph_sacs = {k for k in graph if k.startswith("C")}

    print(f"resolving SAC + routes for {len(leaders)} assets…", file=sys.stderr)
    for asset in leaders:
        asset.sac = resolve_sac(asset.code, asset.issuer)
        time.sleep(args.sleep)
        if not asset.sac:
            asset.error = "no SAC on stellar.expert"
            continue
        asset.in_graph = asset.sac in graph_sacs
        if not asset.in_graph:
            asset.error = "not in LumAgg token graph"
            continue
        if args.skip_quote:
            asset.route_ok = None
            continue
        ok_out, out_amt, err = quote_ok(
            args.quote_api, XLM_SAC, asset.sac, args.amount_in, args.max_hops
        )
        time.sleep(args.sleep)
        if not ok_out:
            asset.route_ok = False
            asset.error = f"no XLM→token: {err}"
            continue
        # Round-trip back needs enough of the bridge token; use quoted out.
        back_in = out_amt or args.amount_in
        ok_back, _, err_b = quote_ok(
            args.quote_api, asset.sac, XLM_SAC, back_in, args.max_hops
        )
        time.sleep(args.sleep)
        if not ok_back:
            asset.route_ok = False
            asset.error = f"no token→XLM: {err_b}"
            continue
        asset.route_ok = True
        asset.amount_out = out_amt

    # Classify
    already: list[RankedAsset] = []
    suggest: list[RankedAsset] = []
    skipped: list[RankedAsset] = []
    for a in leaders:
        if a.sac and a.sac in current:
            already.append(a)
        elif a.route_ok is True or (args.skip_quote and a.in_graph):
            suggest.append(a)
        else:
            skipped.append(a)

    def row(a: RankedAsset, status: str) -> str:
        sac = a.sac or "-"
        vol = f"${a.volume_24h_usd:,.0f}"
        note = a.error or (f"out={a.amount_out}" if a.amount_out else "")
        return f"{status:8} {a.code:8} {vol:>12}  {sac}  {note}"

    print()
    print(f"{'status':8} {'code':8} {'vol_24h_usd':>12}  sac  note")
    print("-" * 100)
    for a in already:
        print(row(a, "HAVE"))
    for a in suggest:
        print(row(a, "ADD"))
    for a in skipped:
        print(row(a, "SKIP"))

    print()
    print(f"summary: HAVE={len(already)} ADD={len(suggest)} SKIP={len(skipped)} current_bridges={len(current)}")
    if suggest:
        print()
        print("# suggested additions (SAC):")
        for a in suggest:
            assert a.sac
            print(f"#   {a.code:8} {a.sac}  vol24h=${a.volume_24h_usd:,.0f}")
        merged = sorted(current | {a.sac for a in suggest if a.sac})
        print()
        print("# merged ARB_BRIDGE_TOKENS= (current ∪ ADD)")
        print("ARB_BRIDGE_TOKENS=" + ",".join(merged))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
