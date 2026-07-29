# Demo video script (~5 min) — SCF Tranche 1 completion form

Upload as **unlisted** YouTube / Loom / Drive. Paste the URL into **Deliverable Verification - Video**.

**Voice-over recommended.** Read [tranche-1-voiceover-script.md](./tranche-1-voiceover-script.md) while recording the browser. The caption-card deck remains an optional silent fallback: [evidence/LumAgg-Tranche1-Demo-Captions.pptx](./evidence/LumAgg-Tranche1-Demo-Captions.pptx).

For a simple spoken version, read [tranche-1-voiceover-script.md](./tranche-1-voiceover-script.md). It uses short sentences and follows the same screen order.

Audience: SCF reviewers verifying D1–D4. Do **not** spend time on arb vault / npm SDK (those are T2/T3).

---

## 最简单的录制方法（带口播）

1. 打开浏览器并按下面顺序准备标签页：Swap、Docs、GitHub evidence、Benchmark、Stats。
2. 在 macOS 按 `⌘⇧5`，选择 **Record Entire Screen**。
3. 点击 `Options`，将 `Microphone` 设为 **MacBook Pro Microphone**，保存位置选
   Desktop；不要选择 `None`、iPhone Microphone 或 ZoomAudioDevice。
4. 先录 10 秒测试：读两句话后停止，打开 `.mov` 确认能听到声音。确认后再正式录制。
5. 正式录制时按 [口播稿](./tranche-1-voiceover-script.md) 朗读并操作浏览器。鼠标移动慢一点，
   在关键内容上停留 2–3 秒。
6. 录制结束后点击菜单栏停止图标。视频不需要音乐、转场或摄像头画面。

如果 `Options` 中没有麦克风或试录仍然无声：打开
`System Settings → Privacy & Security → Microphone`，允许 Screenshot 或你使用的
录制应用访问麦克风，然后退出并重新打开录制应用。

建议只录一条连续视频；操作失误时停两秒再继续即可，不必追求专业剪辑。目标是让审阅者
确认 D1–D4 的证据，不是制作宣传片。

录完后上传到 YouTube Studio，标题可用 `LumAgg — SCF #44 Tranche 1 Demo`，将
Visibility 设为 **Unlisted**。等待处理完成后，用无痕窗口打开分享链接，确认不登录也能
播放，再把链接填入 completion form 的 `Deliverable Verification - Video`。

---

## Pre-flight (do once before record)

1. Browser: clean window, 1920×1080, hide bookmarks bar; English UI if possible.
2. Tabs ready (left → right):
   - https://lumagg.xyz
   - https://lumagg.xyz/docs
   - https://lumagg.xyz/stats
   - https://github.com/Lum-Agg/stellar-dex-agg (or your public repo)
   - Terminal in repo root
3. Terminal pre-run (so live take is fast):
   ```bash
   # D1 — optional, show results file instead of full re-run if slow
   head -40 docs/scf-benchmark-results.md

   # D2 — do not re-run during recording; show the committed external evidence
   ls docs/evidence/d2-integrator-smoke/
   jq '{success, is_split: .data.is_split, routes: (.data.sub_routes | length)}' \
     docs/evidence/d2-integrator-smoke/quote.json
   jq '{success, xdr_prefix: .data.unsigned_tx_xdr[:60]}' \
     docs/evidence/d2-integrator-smoke/build_resp.json
   ```
4. Freighter: connected on mainnet (optional for UI segment). If you skip signing, still show logos / balance / % chips.
5. Pre-warm one quote on the swap page (XLM → USDC) so the live take is snappy.

---

## Title card (optional, 5s)

Text on screen:

> LumAgg — SCF #44 Tranche 1  
> Integrator API · Swap UX · Analytics indexer · Differentiation evidence  
> https://lumagg.xyz · https://api.lumagg.xyz

---

## 0:00–0:25 — Intro

**Show:** lumagg.xyz homepage / swap.

**Optional narration (skip this when using caption cards):**
> This is LumAgg, a Stellar DEX aggregator. Tranche 1 delivers four things: live differentiation benchmarks, an integrator-ready public API, completed swap UX, and an on-chain analytics indexer. I’ll walk through each with live evidence.

---

## 0:25–1:25 — D3 Swap UI (~60s)

**Show:** https://lumagg.xyz — select **XLM → USDC**.

**Do / point:**
1. Token logos visible in the picker / pair row.
2. Connect wallet (or already connected) → **spendable balance** on input token.
3. Click **25% / 50% / 75% / 100%** chips.
4. Open settings and show **slippage**, **Max hops**, and **Max splits**. Explain
   that they control price protection, route length, and parallel route count.
5. Hit quote → if split, highlight **two legs / percentages / DEX names**.
6. Optional: sign a tiny swap and open the **explorer link**. If not signing,
   leave the quote visible for 3 seconds; the D2 section separately proves the
   `build_tx` path.

**Optional narration (skip this when using caption cards):**
> Deliverable 3 closes retail UX gaps: logos from the tokens API, wallet balance, quick amounts, configurable slippage, maximum hops and splits, and an explorer link after submit. Routing still goes through the public quote and build_tx APIs.

---

## 1:25–2:55 — D2 Integrator API (~90s)

**Show A — Docs (20s):** https://lumagg.xyz/docs  
Point to OpenAPI / integrator guide, mention `prefer_soroban=1` and API keys.

**Show B — Terminal smoke (70s):**

```bash
# If re-running live:
USER_G=GDXRRY4HHIERMJBY62B4YJ25V3YNTMEOG3CQRLRHJ3P57Q57CYSJLPI2 \
  ./scripts/integrator-smoke.sh

# Or open committed evidence:
ls docs/evidence/d2-integrator-smoke/
jq '.success, .data.is_split, .data.sub_routes | length' docs/evidence/d2-integrator-smoke/quote.json
jq '.success, .data.unsigned_tx_xdr[:60]' docs/evidence/d2-integrator-smoke/build_resp.json
```

**Point on screen:**
- `success: true`
- `is_split: true` + `sub_routes`
- `unsigned_tx_xdr` prefix (do **not** scroll forever)
- README: external / non-founder `USER_G`

**Optional narration (skip this when using caption cards):**
> Deliverable 2: partners can quote and get an unsigned XDR from docs alone. Here is an external G-address smoke — quote plus build_tx — no founder key. OpenAPI and the integrator guide are linked from the docs site; prefer_soroban excludes Classic when integrators need Soroban-only comparison.

---

## 2:55–3:50 — D1 Differentiation (~55s)

**Show:**
1. `docs/scf-venue-comparison.md` (GitHub or local) — Broker adapter gap / LumAgg venue list.
2. `docs/scf-benchmark-results.md` — show only the top **Tranche 1 reviewer summary** table; do not scroll through the full result matrix.
3. Optional one-liner: `./scripts/scf-benchmark.sh` exists for reviewers to re-run.

**Optional narration (skip this when using caption cards):**
> Deliverable 1 is verifiable differentiation, not marketing slides. We maintain a live Soroswap quote benchmark and a public Broker router-contract comparison — including CLMM coverage Broker’s open router lacks. Reviewers can re-run the benchmark script; results are dated in the repo.

---

## 3:50–4:45 — D4 Analytics indexer (~55s)

**Show:** https://lumagg.xyz/stats

**Point:**
- Daily / recent tx count and volume
- `split_swap` vs `round_trip_swap` if shown
- Per-DEX breakdown

**Then terminal (optional 10s):**
```bash
curl -sS 'https://api.lumagg.xyz/api/v1/stats?format=csv' | head -5
```

**Optional narration (skip this when using caption cards):**
> Deliverable 4 is the production indexer v0: mainnet aggregator invocations, daily volume and tx counts, function breakdown, and per-DEX leg attribution. Dashboard UI polish continues in a later tranche; the data pipeline and /stats export are live now.

---

## 4:45–5:00 — Close (15s)

**Show:** GitHub repo root or end card.

**Say:**
> That’s Tranche 1: benchmarks, integrator API with external smoke evidence, completed swap UX, and live analytics. Links are in the completion form — lumagg.xyz, api.lumagg.xyz, and the repo evidence folder. Thanks for reviewing.

**End card text:**
> https://lumagg.xyz  
> https://api.lumagg.xyz  
> https://github.com/Lum-Agg/stellar-dex-agg  
> Evidence: docs/evidence/d2-integrator-smoke/

---

## Timing cheat-sheet

| Time | Segment | Deliverable |
|------|---------|-------------|
| 0:00 | Intro | — |
| 0:25 | Swap UI | D3 |
| 1:25 | Docs + smoke | D2 |
| 2:55 | Benchmark docs | D1 |
| 3:50 | /stats + CSV | D4 |
| 4:45 | Close | — |

If you run long: cut Freighter signing first, then cut live `scf-benchmark.sh`.

---

## Form paste (video description / Additional Verification)

```text
Tranche 1 demo (~5 min): D3 swap UX (logos, balance, % chips) → D2 integrator docs + external quote/build_tx smoke → D1 benchmark & venue comparison docs → D4 live /stats + CSV export.
Evidence folder: docs/evidence/d2-integrator-smoke/
```

---

## Recording tips

- Use macOS `⌘⇧5` at the display's normal resolution and select `MacBook Pro Microphone`.
- Cursor highlight / zoom on JSON keys (`is_split`, `unsigned_tx_xdr`).
- Don’t open private keys, `.env`, or Telegram bot tokens.
- One rehearsal take, then one real take. A clear screen recording is enough.
