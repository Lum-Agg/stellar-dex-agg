#!/usr/bin/env python3
"""SCF quote benchmark: LumAgg vs optional Soroswap API."""

from __future__ import annotations

import json
import os
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any

LUMAGG_API = os.environ.get("LUMAGG_API", "https://api.lumagg.xyz").rstrip("/")
SOROSWAP_API_URL = os.environ.get("SOROSWAP_API_URL", "https://api.soroswap.finance").rstrip("/")
SOROSWAP_API_KEY = os.environ.get("SOROSWAP_API_KEY", "").strip()
OUTPUT = os.environ.get("OUTPUT", "").strip()

XLM = "CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA"
USDC = "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75"
AQUA = "CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK"

# (label, token_in, token_out, amount_in stroops, size_label)
CASES: list[tuple[str, str, str, int, str]] = [
    ("USDC → XLM", USDC, XLM, 10_000_000, "1 USDC"),
    ("USDC → XLM", USDC, XLM, 100_000_000, "10 USDC"),
    ("USDC → XLM", USDC, XLM, 1_000_000_000, "100 USDC"),
    ("USDC → XLM", USDC, XLM, 10_000_000_000, "1,000 USDC"),
    ("XLM → USDC", XLM, USDC, 10_000_000, "1 XLM"),
    ("XLM → USDC", XLM, USDC, 100_000_000, "10 XLM"),
    ("XLM → USDC", XLM, USDC, 1_000_000_000, "100 XLM"),
    ("XLM → USDC", XLM, USDC, 10_000_000_000, "1,000 XLM"),
    ("XLM → AQUA", XLM, AQUA, 100_000_000, "10 XLM"),
    ("XLM → AQUA", XLM, AQUA, 1_000_000_000, "100 XLM"),
    ("XLM → AQUA", XLM, AQUA, 10_000_000_000, "1,000 XLM"),
]


@dataclass
class LumAggQuote:
    amount_out: int
    is_split: bool
    legs: int
    sources: list[str]
    compute_ms: int | None
    error: str | None = None


@dataclass
class SoroswapQuote:
    amount_out: int | None
    error: str | None = None


def http_get_json(url: str, timeout: float = 60.0) -> Any:
    req = urllib.request.Request(
        url,
        headers={
            "Accept": "application/json",
            "User-Agent": "lumagg-scf-benchmark/1.0 (SCF evidence script)",
        },
    )
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode())


def http_post_json(url: str, body: dict[str, Any], headers: dict[str, str], timeout: float = 60.0) -> Any:
    data = json.dumps(body).encode()
    req = urllib.request.Request(url, data=data, headers=headers, method="POST")
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode())


def lumagg_quote(token_in: str, token_out: str, amount_in: int) -> LumAggQuote:
    params = urllib.parse.urlencode(
        {
            "token_in": token_in,
            "token_out": token_out,
            "amount_in": str(amount_in),
            "slippage": "0.5",
            "debug": "1",
        }
    )
    url = f"{LUMAGG_API}/api/v1/quote?{params}"
    try:
        raw = http_get_json(url)
    except urllib.error.URLError as e:
        return LumAggQuote(0, False, 0, [], None, error=str(e))
    if not raw.get("success"):
        return LumAggQuote(0, False, 0, [], None, error=str(raw.get("error", raw)))
    data = raw["data"]
    routes = data.get("sub_routes") or []
    sources = [r.get("source", "?") for r in routes]
    return LumAggQuote(
        amount_out=int(data["expected_output"]),
        is_split=bool(data.get("is_split")),
        legs=len(routes),
        sources=sources,
        compute_ms=data.get("compute_time_ms"),
    )


def soroswap_quote(token_in: str, token_out: str, amount_in: int) -> SoroswapQuote:
    if not SOROSWAP_API_KEY:
        return SoroswapQuote(None, error="SOROSWAP_API_KEY not set")
    url = f"{SOROSWAP_API_URL}/quote?network=mainnet"
    body = {
        "assetIn": token_in,
        "assetOut": token_out,
        "amount": str(amount_in),
        "tradeType": "EXACT_IN",
        "protocols": ["soroswap", "phoenix", "aqua"],
    }
    headers = {
        "Authorization": f"Bearer {SOROSWAP_API_KEY}",
        "Content-Type": "application/json",
        "Accept": "application/json",
        "User-Agent": "lumagg-scf-benchmark/1.0 (SCF evidence script)",
    }
    try:
        raw = http_post_json(url, body, headers)
    except urllib.error.HTTPError as e:
        try:
            detail = json.loads(e.read().decode())
        except Exception:
            detail = e.reason
        return SoroswapQuote(None, error=f"HTTP {e.code}: {detail}")
    except urllib.error.URLError as e:
        return SoroswapQuote(None, error=str(e))

    amount_out = raw.get("amountOut") or raw.get("amount_out") or raw.get("expectedOutput")
    if amount_out is None:
        return SoroswapQuote(None, error=f"unexpected response keys: {list(raw.keys())}")
    return SoroswapQuote(int(amount_out))


def outputs_comparable(lumagg_out: int, soroswap_out: int | None) -> bool:
    """Flag rows where APIs likely use different route classes (Classic vs Soroban) or bad quotes."""
    if soroswap_out is None or soroswap_out <= 0 or lumagg_out <= 0:
        return True
    ratio = lumagg_out / soroswap_out if lumagg_out >= soroswap_out else soroswap_out / lumagg_out
    return ratio <= 3.0


def fmt_stroops(v: int) -> str:
    return f"{v / 10_000_000:,.4f}"


def pct_delta(lumagg: int, other: int | None) -> str:
    if other is None or other <= 0:
        return "—"
    delta = (lumagg - other) / other * 100.0
    sign = "+" if delta >= 0 else ""
    return f"{sign}{delta:.2f}%"


def unique_sources(sources: list[str]) -> str:
    seen: list[str] = []
    for s in sources:
        if s not in seen:
            seen.append(s)
    return ", ".join(seen) if seen else "—"


def notes_for_case(pair: str, lumagg: LumAggQuote) -> str:
    if lumagg.error:
        return lumagg.error
    parts: list[str] = []
    if "classic_dex" in lumagg.sources:
        parts.append("LumAgg picked Classic DEX (Soroswap column is Soroban+SDEX mix — interpret carefully)")
    if any("clmm" in s or s == "sushi" for s in lumagg.sources):
        parts.append("CLMM venue in route")
    if lumagg.is_split:
        parts.append(f"split {lumagg.legs} legs")
    return "; ".join(parts) if parts else "—"


def run_benchmark() -> str:
    ts = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    has_soroswap = bool(SOROSWAP_API_KEY)

    lines: list[str] = [
        "# LumAgg quote benchmark results",
        "",
        f"Generated: **{ts}**",
        "",
        f"- LumAgg API: `{LUMAGG_API}`",
        f"- Soroswap API: `{SOROSWAP_API_URL}` "
        + ("(key provided)" if has_soroswap else "(**no API key** — Soroswap column empty)"),
        "",
        "Reproduce:",
        "",
        "```bash",
        "./scripts/scf-benchmark.sh",
        "# With Soroswap:",
        "SOROSWAP_API_KEY=sk_... OUTPUT=docs/scf-benchmark-results.md ./scripts/scf-benchmark.sh",
        "```",
        "",
        "> **Interpretation:** USDC→XLM and XLM→AQUA exercise Soroban pools (often Aquarius CLMM). "
        "XLM→USDC may route via Classic DEX when SDEX path wins — not apples-to-apples vs Soroswap Soroban-only. "
        "Positive Δ = LumAgg higher output for same `amount_in`.",
        "",
    ]

    if has_soroswap:
        header = (
            "| Pair | Size | LumAgg out | Split | Sources | Soroswap out | Δ vs Soroswap | Notes |"
        )
        sep = "|------|------|------------|-------|---------|--------------|---------------|-------|"
    else:
        header = "| Pair | Size | LumAgg out | Split | Sources | Soroswap out | Notes |"
        sep = "|------|------|------------|-------|---------|--------------|-------|"

    lines.extend([header, sep])

    for pair, token_in, token_out, amount_in, size_label in CASES:
        lumagg = lumagg_quote(token_in, token_out, amount_in)
        time.sleep(0.12)  # api-server IP limiter: 10 req/s
        soroswap = soroswap_quote(token_in, token_out, amount_in)
        note = notes_for_case(pair, lumagg)

        if lumagg.error:
            lumagg_out = "ERR"
            split = "—"
            sources = "—"
        else:
            lumagg_out = fmt_stroops(lumagg.amount_out)
            split = "yes" if lumagg.is_split else "no"
            sources = unique_sources(lumagg.sources)

        if soroswap.error or soroswap.amount_out is None:
            ss_out = "—" if not has_soroswap else f"ERR"
            if has_soroswap and soroswap.error:
                note = f"{note}; Soroswap: {soroswap.error}" if note != "—" else f"Soroswap: {soroswap.error}"
        else:
            ss_out = fmt_stroops(soroswap.amount_out)

        if has_soroswap:
            if soroswap.amount_out is not None and not lumagg.error:
                if not outputs_comparable(lumagg.amount_out, soroswap.amount_out):
                    note = (
                        f"{note}; ⚠️ outputs not comparable (>3× gap — Classic DEX vs Soroban mix)"
                        if note != "—"
                        else "⚠️ outputs not comparable (>3× gap — Classic DEX vs Soroban mix)"
                    )
                    delta = "n/a"
                else:
                    delta = pct_delta(lumagg.amount_out, soroswap.amount_out)
            else:
                delta = pct_delta(lumagg.amount_out if not lumagg.error else 0, soroswap.amount_out)
            lines.append(
                f"| {pair} | {size_label} | {lumagg_out} | {split} | {sources} | {ss_out} | {delta} | {note} |"
            )
        else:
            lines.append(
                f"| {pair} | {size_label} | {lumagg_out} | {split} | {sources} | {ss_out} | {note} |"
            )

    lines.extend(
        [
            "",
            "## Summary",
            "",
            "- **Venue coverage:** See [scf-venue-comparison.md](scf-venue-comparison.md) for Stellar Broker CLMM gap (source-based).",
            "- **Split routing:** LumAgg `is_split=true` when Brent optimizer splits across paths; Soroswap API returns a single best route.",
            "- **Soroswap API key:** Free registration at https://api.soroswap.finance/register — re-run this script before SCF resubmission to fill the comparison column.",
            "",
        ]
    )

    return "\n".join(lines) + "\n"


def main() -> None:
    md = run_benchmark()
    print(md, end="")
    if OUTPUT:
        out_path = OUTPUT
        os.makedirs(os.path.dirname(out_path) or ".", exist_ok=True)
        with open(out_path, "w", encoding="utf-8") as f:
            f.write(md)
        print(f"\nWrote {out_path}", file=sys.stderr)


if __name__ == "__main__":
    main()
