# Entitlement inventory for the licensed-sample ask

Research date: 2026-08-28 (Asia/Seoul)

## Verdict

The repository already holds **three free, live entitlements** — the LS xing paper
lanes, the KRX Open API with a **per-service 이용신청 approved for six equity
services**, and the KASI Special Day Information service. Between them they can
answer, at zero cost, **six of the nineteen obligations** in
[`krx-licensed-sample-request-package.md`](krx-licensed-sample-request-package.md) §2
— and two of those six are already answered on production evidence.

The bound is structural, not administrative. Every free surface the repo holds is
**daily-aggregate, currently-listed equity data**. So it can move the *universe*
obligations (1, 2, 3, 4, 12, 16) and can never move the *fidelity* obligations
(6–11, 14) or the *contractual* ones (17–19), no matter how many free
applications are filed. The paid ask is therefore irreducible on the fidelity and
contract axes, and the two `shape`-determining rows — 17 and 18 — are unreachable
from any entitlement by construction: only a counterparty can answer them.

**One free act remains untaken and would shrink the paid ask further.** A backward
`basDd` walk over the three already-approved base-info services would *measure*
obligations 1, 2 and 4 rather than buying them. It is free, operator-runnable, and
recorded here so it is not rediscovered as a paid question. This is the same
finding one generation earlier in
[`vendor-sample-endpoint-evidence-describes-the-sample-not-the-product.md`](../solutions/conventions/vendor-sample-endpoint-evidence-describes-the-sample-not-the-product.md):
*a blocker stated as "needs credentials" deserves one grep for credentials the tree
already has.*

**No ceiling input may rest on a completeness property.** Base-info completeness is
`UNESTABLISHED` and this inventory does not discharge it: base-info and 일별매매정보
agree exactly per market (943 / 1,820 / 109 both ways) but both are KRX-sourced, so
that is internal consistency, not independent corroboration.

## The three-verdict vocabulary, and why it had to be authored

The request package and the Part A findings use a five-label scheme — `SETTLED`,
`PARTIALLY SETTLED`, `NOT SETTLED`, `NEEDS VENDOR`, `NOT RELEASED` — plus
`UNESTABLISHED` for completeness and `[PRIMARY]` / `[SECONDARY]` for source
authority. That scheme records *how much of an obligation moved*. It does not
record *why it did not move*, which is the distinction that governs whether a
question belongs in a paid ask at all.

The three verdicts below come from the sample-endpoint convention's rule:

> Separate three verdicts and never collapse them: *the source says X*, *the source
> is silent on X*, and *I could not reach the document that would say*. Conflating
> the second and third is how a research gap gets recorded as a vendor limitation.

| Verdict | Means | Consequence for the ask |
|---|---|---|
| **measured** | Established by a call we made or a document we read, on evidence this repository holds. | Strike the question. Cite the evidence. |
| **source-silent** | We reached the authoritative document and it does not answer. | Keep the question. It is a genuine vendor ask. |
| **unreachable** | We could not reach the document that would answer, or no such document is published. | Keep the question, and say so — this is a research gap, not a vendor limitation. |

The two schemes are **not isomorphic**, and the mapping is per-row rather than
mechanical:

- `SETTLED` → **measured** in every case.
- `PARTIALLY SETTLED` splits: the settled part is **measured**, the residue is
  **source-silent** when the authority was read and silent (obl. 3's KOSPI-blind
  SPAC field) and **unreachable** when the deciding document was never reached
  (obl. 12's undated regime changes).
- `NOT SETTLED` / `NEEDS VENDOR` → **source-silent** when a public page was read
  and did not carry the fact, **unreachable** when no public page exists.
- `NOT RELEASED` → **source-silent**: the licence text was read and answers
  adversely-by-omission, which is a finding rather than a gap.
- `UNESTABLISHED` has **no slot here** and deliberately keeps its own label. It is
  a property of a *census we have not run*, not of a source — and R26 forbids any
  ceiling input resting on it.

## Entitlement 1 — LS Securities xing gateway, paper lanes

| Property | Finding |
|---|---|
| Cost | Free. Paper lanes only; `LS_TRADING_ENV=paper` is required by the runtime. |
| Credential shape | Per-account appkey / secret / account triple, one gitignored file per instrument domain. Variable names `LS_TRADING_ENV`, `LS_PAPER_APIKEY` (legacy alias `LS_PAPER_APPKEY`), `LS_PAPER_SECRET`, `LS_PAPER_ACCOUNT`. |
| Where recorded | `.env.example`; `AGENTS.md` § Live smokes & gateway; `Makefile`'s `live-smoke-*` targets via `LS_SMOKE_LANE`. Lane files present: `.env`, `.env.domestic`, `.env.domestic_option`, `.env.overseas`, `.env.overseas_option`, `.env.sparse`. |
| What it grants | The TR read surface — daily and N-minute bars (t8410, t8412), universe skeleton (t8430), per-board market-cap rank (t1444). |
| Measured bound | Call budget is `per_credential` with an **unmeasured ceiling** — `adapters/nautilus/lab/config/gateway-budget.json` records `bucket_scope: per_credential`, `budget_calls: null`, provenance "MEASURED via budget-probe 2026-07-09". |
| Obligations it can move | **None.** It is the supply this arc exists to replace: the rolling minute window is ~237 KRX sessions, which is the wall the closure rule evaluated against. |

**Verdict: measured.** Its scope and its ceiling are both established, and the
established fact is that it cannot answer any obligation in the register.

## Entitlement 2 — KRX Open API, six equity services approved

| Property | Finding |
|---|---|
| Cost | Free. Key plus a **per-service 이용신청** approval; 10,000 calls/day; one-year renewable term; key deleted after 12 months of non-use. |
| Credential shape | `LS_KRX_APPKEY`, in the gitignored `.env.calendar`. |
| Where recorded | `adapters/nautilus/RUNBOOK-calendar-snapshot.md`; `RUNBOOK-session-morning.md`; `adapters/nautilus/src/calendar_refresh/transport.rs`; `scripts/krx-witness-watch.sh`; `scripts/session-morning.sh`. |
| Approved services | `sto/stk_isu_base_info`, `sto/ksq_isu_base_info`, `sto/knx_isu_base_info`, `sto/stk_bydd_trd`, `sto/ksq_bydd_trd`, `sto/knx_bydd_trd` — all six verified live on the **production** path at `basDd=20260803`, returning 943 / 1,820 / 109 rows. See the 2026-08-04 addendum in [`krx-part-a-public-pass-findings.md`](krx-part-a-public-pass-findings.md). |
| Scope ceiling | **Daily-aggregate, currently-listed equity only.** The base-info record is 12 fields; 일별매매정보 is one row per issue per session. Nothing order-level, nothing intraday, no delisted issues, no constituent data. |
| Terms ceiling | Non-commercial only; no provision of KRX data to third parties; no post-expiry use; `한국거래소 통계정보` attribution required. #243's standing finding — **KRX Open API terms are non-commercial and unusable in production** — bounds this entitlement to research, permanently. |
| Publication ceiling | Redistribution of even a *derived* date/status table is **not** authorized by the published writing — see [`krx-calendar-publication-rights.md`](krx-calendar-publication-rights.md). |

**Path trap, already recorded and repeated here because it reads as an entitlement
failure:** the market group in the path is `sto` for *all* equity services,
including KOSDAQ and KONEX. `ksq/ksq_isu_base_info` returns
`404 {"respMsg":"API referenced by the path does not exist."}`, which at a glance is
indistinguishable from an unapproved service. Vary the path before concluding
anything about entitlement.

## Entitlement 3 — KASI Special Day Information

| Property | Finding |
|---|---|
| Cost | Free. Development and production keys auto-approved; 10,000 development requests. |
| Credential shape | `LS_KASI_SERVICE_KEY`, in the gitignored `.env.calendar`. |
| Where recorded | [`krx-calendar-forward-closures-api.md`](krx-calendar-forward-closures-api.md); `scripts/session-morning.sh`. |
| What it grants | Government public-holiday dates via `getRestDeInfo`. |
| Scope ceiling | Not KRX-sourced, and does not cover discretionary KRX closures. It is a holiday oracle, not a session oracle. |
| Obligations it can move | **None** in the register. It bounds R13's unknown-calendar-day residual rather than any licensed-sample obligation. |

**Verdict: measured.** Established free, established insufficient alone.

## Non-entitlements, recorded so they are not assumed

| Counterparty | Status | Verdict |
|---|---|---|
| **Koscom** (KRX Securities A/B/C feeds) | No credential, no wiring, no application filed. Its 접속표준서 is **publicly downloadable**, which is how obligation 15 was struck without any entitlement at all. | **measured** — the public spec answered the acquisition gate; the feed itself is a paid ask. |
| **OpenDART** | No credential, no wiring. Appears only as a prospective source in the unmerged #243 feasibility note. | **unreachable** — no application filed, so nothing was read. |
| **KIND** | No credential, no wiring. Named once, negatively: polling an undocumented KIND or KRX web endpoint "would convert the design into scraping". | **measured** — deliberately excluded by design, not blocked by entitlement. |

## Obligation register, with an entitlement verdict per row

Every row in the request package's §2 register, with the R26 verdict on its current
evidence and whether a **free** entitlement this repository already holds could move
it. "Free act available" names an act nobody has run yet; it is the column that
shrinks the paid ask.

| # | R26 verdict | Free entitlement can move it? | Basis |
|---|---|---|---|
| 1 | source-silent | **Yes, partially — free act available** | The approved base-info services carry `ISU_CD` (ISIN) and `ISU_SRT_CD` (shcode) keyed by `basDd`. A backward `basDd` walk measures code change and reassignment directly for issues listed inside the walked window. Reassignment *after* delisting stays out of reach: delisted issues are absent from the master. |
| 2 | source-silent | **Yes, partially — free act available** | The same walk yields an effective-dated ISIN↔shcode alias table for every issue listed at any point in the window. Inactive issues predating the window remain a paid ask. |
| 3 | measured (with a source-silent residue) | **Yes — already done** | Whole-market at n=2,872: `SECUGRP_NM` separates REIT / foreign / DR; `KIND_STKCERT_TP_NM` separates 보통주 from preferred (113 non-보통주); `SECT_TP_NM` carries `SPAC(소속부없음)` (68 issues). Residue is **source-silent**: `SECT_TP_NM` is empty for all 943 KOSPI issues, and status displaces class on 3 관리종목 SPACs. |
| 4 | source-silent | **Yes — free act available** | The proportion of symbols whose as-of class is unestablished is countable from the walked base-info records. It needs no vendor. |
| 5 | measured (public half) | **Yes — already done, public** | 가격제한폭 is expressly not applied to 정리매매종목 (업무규정 제20조제3항). F6 has no value to compute because the quantity is undefined by regulation, not missing from a feed. |
| 6 | unreachable | No | ETF PDF apply-date history is outside the free surface's daily-aggregate equity scope. Pre-decided by #243: remove the feature rather than reconstruct with hindsight. |
| 7 | source-silent | No | ST5001/ST5002 are order-level products. Whether they carry auction-phase identifiers, indicative price or imbalance "is not on any public page". |
| 8 | source-silent | No | VI trigger and release timestamps are event-grained; no free surface carries them. |
| 9 | source-silent | No | F9's event-grained stream has **no other source**. |
| 10 | measured as *narrowed*, source-silent on the answer | No | ST5001 is order-level MBO, millisecond, with a synchronized 10-level book on every event (104 fields). A product that can answer obligation 10 exists and is priced; whether it *does* is a vendor claim, not verified data. |
| 11 | source-silent | No | Correction and re-delivery semantics are per-product contract terms. |
| 12 | measured (with an unreachable residue) | **Yes — already done, public** | 15 in-window regime changes enumerated with effective dates from KRX's own change register. The load-bearing one — **regular-session close 15:00 → 15:30 on 2016-08-01** — arms the `KRX_REGULAR_CLOSE` defect this plan's U9 closes. Undated remainder is **unreachable**, not source-silent: the primary notice was not reachable. |
| 13 | source-silent | Partially | The free services' own update behaviour is measured in the calendar research; per-paid-product cadence is not. Pre-decided by M12: no declared cadence ⇒ `Unevaluated` ⇒ the feature drops. |
| 14 | measured (taxonomy), source-silent (per-product mapping) | No | The taxonomy is fully public; **no public per-trade category code exists** and the Open API is daily-aggregate only. Odd-lot is struck — the trading unit has been 1 share since 2014-06-02. |
| 15 | **measured — gate passes** | **Yes — already done, public** | Sequence number (`정보분배일련번호`, field #3, every message) **and** heartbeat (UDP `I2000`; TCP `Link` at 1 minute) are both in Koscom's publicly downloadable 접속표준서. Question struck outright, with no entitlement required. |
| 16 | **measured — negative** | **Yes — already done** | 업종 is not served by the base-info services and appears nowhere in the ~40-service catalogue. Confirmed on production at n=2,872: the 12-field record carries no industry classification, and `SECT_TP_NM`'s values are board sections plus status labels. **업종 stays its own acquisition line; M17's near-zero-marginal-cost premise stays false.** |
| 17 | source-silent (**`shape`**) | **No — unreachable from any entitlement** | Route B (KRX historical, the operative regime) pulls 가공한 정보 inside the restriction and offers only a discretionary 독창성 test a mechanical digest may fail. Only a counterparty can release this. `may_begin` stays refused. |
| 18 | source-silent (**`shape`**) | **No — unreachable from any entitlement** | Route B defines no term, no 해지 and no expiry, yet Art. 11(5) makes KRX's destruction-demand power survive indefinitely with no reciprocal right, and Art. 9(3) reaches derived information. The asymmetry is the finding. |
| 19 | source-silent | No | Zero hits for 에스크로 / 임치 across all 15 Route B articles and both language versions of Route A's Policy. Absence from published terms cannot prove an unpublished offering does not exist, so this stays a one-question ask. |

**Count.** Six rows a free entitlement can move: 3, 5, 12, 15 and 16 are **already
moved**; 1, 2 and 4 are moveable by the one untaken free act (and 13 partially).
Eleven rows are irreducibly paid. Two rows — 17 and 18 — are unreachable from every
entitlement that exists, which is why `may_begin` is still false and why no amount
of free work shortens the arc's head.

## The one untaken free act

**A backward `basDd` walk over `sto/{stk,ksq,knx}_isu_base_info`.** Free, inside the
existing approval, operator-runnable, bounded by the 10,000-calls/day quota. It
would convert obligations 1, 2 and 4 from paid asks into measured facts for the
walked window, and it costs nothing but wall-clock.

Two limits to declare before running it, so its output is not over-read:

1. **It cannot see delisted issues.** The master is a listed-issue snapshot per
   `basDd`, so reassignment *after* delisting — the sharp half of obligation 1 and
   #256's actual decider — stays outside it.
2. **It does not establish completeness.** Walking more dates multiplies the same
   KRX-sourced evidence; it is not an independent census. Completeness stays
   `UNESTABLISHED`.

## Required facts that remain unestablished

- **Base-info completeness.** Needs a production call *plus* reconciliation against
  an independent listed-issue census. The first is done; the second is not. No
  ceiling input may rest on this (R26).
- **The KRX Open API call-budget ceiling** is `null` in `gateway-budget.json` — the
  quota is documented at 10,000/day but the repo has never probed the enforcement
  boundary, so a long walk's failure mode is unmeasured.
- **Whether a KOSPI SPAC is identifiable at all** from any free surface.
  `SECT_TP_NM` is empty for every KOSPI issue, so the discriminator that works on
  KOSDAQ is structurally blind on the larger board.

## Decision gate

The sample ceiling may be frozen once this inventory is read, because it discharges
R25's precondition: no paid question in the ask is one a free entitlement already
answers. Obligations 3, 5, 12, 15 and 16 are struck or narrowed on free evidence,
and the untaken free act is *recorded rather than required* — it would shrink the
ask further, but it cannot move rows 17 or 18, so it does not gate the ceiling.

A reply from a counterparty that only says "the licence permits research use" is
insufficient for rows 17 and 18: it must classify the described transformation
under the operative articles and answer publication of a content hash and
post-termination retention of derived evidence **separately**, because either
answer is usable and only silence is not.
