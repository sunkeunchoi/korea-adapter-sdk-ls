# Templates — evidence file & recommendation block

Mirror the in-repo exemplars exactly: `metadata/evidence/t1101.yaml`,
`metadata/evidence/token.yaml`, `metadata/trs/t1101.yaml`.

## Evidence file (`metadata/evidence/<tr>.yaml`)

```yaml
# Focused Evidence — <tr> (<short human description>)
#
# A durable, credential-free record of a genuine Paper Live Smoke. The `line`
# below is captured VERBATIM from the `LIVE-SMOKE` stdout of `make <target>`
# (crates/ls-sdk/tests/live_smoke.rs::<fn>), which is credential-free and
# self-dated by construction (no rsp_msg, structural fields only).
#
# Secret-safety: structural descriptors and public market data only — never the
# token, appkey, secret, or account number.
#
# Integrity of THIS record: <why the rsp_cd + structural fields prove a genuine
# round-trip and not a guard miss / paper-incompatible refusal>.
tr_code: <tr>
date: <YYYY-MM-DD today, == maintenance.last_reviewed>
env: paper
target: <make target>
line: "<verbatim LIVE-SMOKE line>"
```

**Date formats differ by target** (match what the smoke emits): the default/book
quote stamps `%Y-%m-%d`; the chart smoke's input date is `%Y%m%d`. The evidence
`date:` field is always `%Y-%m-%d` and must equal `maintenance.last_reviewed`.

## Recommendation block (in `metadata/trs/<tr>.yaml`)

```yaml
recommendation:
  behavior: <exactly what the smoke exercised — paper, single symbol/page/inquiry/lifecycle>
  evidence_ref: evidence/<tr>.yaml
  excludes:
    - <capability the smoke did NOT prove>
    - ...
    - Production-credential <variant> (evidence is paper only)
    - Stronger freshness automation than the stated policy (no enforcement in code today)
```

### Worked `excludes` per class (the scoping the smoke earns)

**market data — `t1101` / `t1102`:**
```yaml
    - Production-credential quotes (evidence is paper only)
    - Fields outside the modeled subset (<price header / 10 levels>)
    - Quote/order-book correctness during halts/VI or outside KRX regular session
    - Stronger freshness automation than the stated policy (no enforcement in code today)
```

**paginated — `t8412`:**
```yaml
    - Production-credential charts (evidence is paper only)
    - Multi-page chart_all correctness beyond a single page
    - Chart correctness on halts/VI days or outside KRX regular session
    - Stronger freshness automation than the stated policy (no enforcement in code today)
```

**account — `CSPAQ12200`:**
```yaml
    - Order- or position-mutating account state (read-only inquiry only)
    - Production-credential account inquiries (evidence is paper only)
    - Balance-value correctness beyond a successful inquiry round-trip
    - Stronger freshness automation than the stated policy (no enforcement in code today)
```

**realtime — `S3_` (lifecycle-scoped — the narrowest, do not overstate):**
```yaml
    - Trade-data correctness (the smoke proves lifecycle, not row contents)
    - In-session row delivery guarantees (a row is bonus, not required)
    - Reconnection semantics after a dropped connection
    - Production-credential feeds (evidence is paper only)
    - Stronger freshness automation than the stated policy (no enforcement in code today)
```

## Credential-free line shapes (what the smoke must emit)

Safe — only lengths, business codes, public tickers/ports, structural counts:
```
LIVE-SMOKE target=live-smoke      inputs=[env=paper symbol=005930 date=2026-06-17] result=[token_len=380 rsp_cd=00000 price=346500]
LIVE-SMOKE target=live-smoke-chart inputs=[symbol=005930 date=20260617]            result=[rsp_cd=00000 rows=381]
LIVE-SMOKE target=live-smoke-account inputs=[balcretp=1]                            result=[rsp_cd=00136 reccnt=1]
LIVE-SMOKE target=live-smoke-ws    inputs=[symbol=005930 ws_port=29443]            result=[no row within timeout (not a failure)]
```
Never emit `rsp_msg` (localized, account-identifying) or any token/appkey/secret/
account number. A failed account run emits a non-`LIVE-SMOKE` `SMOKE-FAIL` line so
nothing capturable leaks.
