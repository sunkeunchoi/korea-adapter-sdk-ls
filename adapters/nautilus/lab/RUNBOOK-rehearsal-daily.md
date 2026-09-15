# Runbook — daily paper rehearsal (the attended 15:00–15:33 session)

The procedure for opening and closing one **paper rehearsal session** of the daily-resolution
strategy: the morning chain that prepares the rehearsal home, the attended mount, the 15:20
decision, the 15:30 closing auction, the 15:33 teardown, and what to do when any of it refuses.

This is the rehearsal's **single entry document** (plan `2026-09-08-1215` § Documentation).
[`RUNBOOK-rung1.md`](RUNBOOK-rung1.md) remains the ladder path and says nothing about rehearsals;
[`RUNBOOK-session-morning.md`](RUNBOOK-session-morning.md) is the same morning script's **ORB**
profile. Companion: [`README.md`](README.md) § Paper rehearsal.

> **Why a rehearsal is not a ladder session.** It runs on the paper lane under the same safety
> envelope as a mount, but with **no dispatch chain at all** — nothing is consumed, nothing is
> appended, no rung evidence is produced, and its sessions count toward no rung's `N`
> (`src/runner/live/rehearsal.rs:1-19`). What replaces the ladder's peek/consume is the trip gate,
> the mount cutoff, and a pre-build book probe. A rehearsal exists to falsify the **driver**, not
> to earn evidence.

> ⚠️ **`make next` cannot see this.** The work queue's attended-chain display does not know the
> `daily-rehearsal` profile (plan § Documentation). Until that integration lands, this runbook and
> the rehearsal ledgers under `<data_home>/rehearsal/` are the only status surface.

---

## The day's order

| KST | what | who |
|---|---|---|
| — | source credentials, refresh the keepalive | operator |
| by **15:00** | the morning chain's ingest is DONE (`LS_SM_INGEST_BY`) | `session-morning.sh` |
| by **15:10** | the daily universe is IN HAND (`LS_SM_UNIVERSE_BY`) | `session-morning.sh` |
| before **15:15** | `lab-live --rehearse-daily` mounts (`mount_cutoff_kst`) | operator |
| 15:00–15:20 | the pre-decision sweep polls t8407 every 20 s, establishing the marks the limit policy prices against | driver |
| **15:20** | the decision bar — t8407's synthetic 1-DAY row reaches `on_bar` (`decision_kst`, KTD4) | driver |
| 15:20–15:30 | orders rest in the closing single-price auction as marketable limits (k = 3 ticks, KTD5) | driver |
| **15:30** | the auction clears; every crossing limit fills **at the close** (`auction_end_kst`) | KRX |
| **15:33** | the driver requests the node's stop; fail-closed teardown, then finalize (`session_end_kst`) | driver |
| after **15:40** | book adoption is permitted again (see § Recovery) | operator |

All times come from [`config/rehearsal-envelope.json`](config/rehearsal-envelope.json). That file is
**operational**, not a governance artifact — changing a value re-bases nothing, because a rehearsal
produces no rung evidence. The ladder's `config/preregistration.json` is untouched by this lane.

**Stay at the terminal.** The operator keepalive file's mtime is the operator dead-man feeder; a
stale one trips the session (§ Recovery).

---

## Preconditions

Check these **before the first session**, and re-check the starred ones each day.

| # | precondition | how to verify |
|---|---|---|
| 1 | The rehearsal home exists | `ls data/rehearsal-daily/{catalog,rehearsal,state}` — created once by [`../scripts/rehearsal-bootstrap.sh`](../scripts/rehearsal-bootstrap.sh) |
| 2 | The paper account is **flat**, and stays that way until the first session | `make r32-hold-verify` reads t0424 read-only |
| 3 | D+2 deposit ≥ **100,000,000 KRW** ★ | the mount's own preflight enforces it (`min_deposit_krw`); below it the mount is 71 |
| 4 | The credential lane file exists ★ | `.env.domestic` (or `.env.<LS_LANE>`), `0600`, gitignored |
| 5 | `.env.calendar` exists, for the morning chain's KRX/KASI keys ★ | `[ -n "$LS_KRX_APPKEY" ] && [ -n "$LS_KASI_SERVICE_KEY" ]` after sourcing |
| 6 | The lane's **flat-gate harness is stood down** | see below — this one is destructive if missed |
| 7 | The keepalive file exists ★ | `touch` it before mounting; an absent one reads as stale and trips on the first watchdog tick |

### 6 — standing down the lane's flat-gate harness

During the rehearsal the account is **not flat** — that is the point. Everything on `.env.domestic`
that asserts flatness, or restores it, must stand down for the whole rehearsal period (plan `:197`,
`:706`):

| stand down | why |
|---|---|
| **`make paper-reset`** | 🔴 **It cancels every resting order and sells every long position at the daily floor.** Run during a rehearsal it liquidates the inherited book the next session is built on. There is no undo. |
| `make live-smoke-order`, `make live-smoke-order-chain` | pre-assert flat; they will report NOT flat by design |
| `make live-smoke-cspat00601-negative`, `live-smoke-cspat00601-booking-ab`, `live-smoke-cspat00701-igw00000-ab` | flat-gated order placements on the same lane |
| `lab-live --dispatch` (the **ladder's** gate) | its non-deferrable `flat_start` check reds on any open position |

The Makefile already carries this warning for the R32 overnight hold (`Makefile:175-178`); a
rehearsal is the same hazard held open for weeks. Whether a second paper credential could give the
rehearsal its own `.env.rehearsal` lane, removing the conflict entirely, is an **Outstanding
Question** in the plan (`:208`) — until it is answered, the stand-down is the mitigation.

### Step 0 — source the credentials (once, at the top of the session shell)

```sh
R=/ABSOLUTE/path/to/repo
set -a; . "$R/.env.calendar"; set +a          # morning chain: KRX + KASI
[ -n "$LS_KRX_APPKEY" ] && [ -n "$LS_KASI_SERVICE_KEY" ] && echo "calendar credentials loaded"
```

The paper lane's own credentials are **not** sourced into the shell — `lab-live` reads the lane file
itself (`LS_DISPATCH_LANE_ENV`, else `.env.<LS_LANE>`, default `.env.domestic`). Credentials live in
those two gitignored `0600` files and nowhere else: not in this runbook, not in an argument, not in
an observation record, not in a ledger row.

---

## Step 1 — the morning chain (`LS_SM_PROFILE=daily-rehearsal`)

The same [`../scripts/session-morning.sh`](../scripts/session-morning.sh) the ORB head uses, with
the daily profile's defaults. It advances the rehearsal catalog and resolves the universe; it
**never** runs `lab-live` (`--dry-run` asserts that structurally).

```sh
cd adapters/nautilus
LS_SM_PROFILE=daily-rehearsal \
LS_SM_SESSION_DATE=<the PREVIOUS trading session> \
LS_SM_MOUNT_DATE=<today> \
  ./scripts/session-morning.sh --dry-run     # print the resolved sequence, zero traffic
```

> ⚠️ **Always pass `LS_SM_SESSION_DATE`.** Its default is a hardcoded stale literal. A run against
> the default ingests nothing useful and the step [11] watermark row will NO-GO — the watermark
> check reads against the **proven calendar**, not against `LS_SM_SESSION_DATE`, precisely so a
> stale date cannot pass.

What the daily profile changes from the ORB profile (`session-morning.sh:27-40`):

| | orb | daily-rehearsal |
|---|---|---|
| data home | `data/turn4-fresh` | `data/rehearsal-daily` |
| ingest / universe clocks | 09:05 / 09:10 | **15:00 / 15:10** |
| coverage floor | `20260518` | `20160801` |
| step [10] | live t8407 open, waits for 09:00 | **offline `lab-mount-universe --daily`** — ranks and ATR(1) come from the prior session's daily bars, so there is no open to fetch and no pre-auction t8407 hazard |
| step [11] | GO/NO-GO | GO/NO-GO **plus** three conditions (below) |

Step [11]'s three extra conditions:

1. **Every daily watermark has reached the previous proven trading session.** The previous session
   is asked of the real calendar view one day at a time, newest first, bounded to 31 days — never
   re-derived from the snapshot's rows.
2. **`rehearsal/book.json` parses and is not stamped before that session.** A Monday reads Friday's
   book. A stamp *between* the last proven session and today is healthy — the KRX witness is
   retrospective, so yesterday's own teardown routinely stamps a day the calendar has not proven yet.
3. **Any held symbol whose adjustment basis shifted is a WARNING row** (not a NO-GO).

Exit codes — **the contract; never read success from log text**:

| exit | meaning | is a universe in hand? |
|---|---|---|
| `0` | **GO** — universe resolved, report delivered | yes |
| `1` | **NO-GO** — a step failed in a way this runbook anticipates | no |
| `40` | **STAND-DOWN** — not on pace; abandoned before the universe step | no |
| `41` | **CATCH-UP COMPLETE** — `--catch-up` only; universe refused by design | no, and none was wanted |
| `64` | misconfiguration, or a missing/stale/unevaluable binary — refused before any traffic | no |

There is **no valid non-zero outcome** on the daily path's step [10]: the ORB producer's flat-open
`0` refusal has no daily counterpart, so anything but `rc 0` with a written file is NO-GO. Both
profiles refuse (`64`) a data home carrying the `FROZEN-20260812` marker — the judgment home must
never be advanced.

---

## Step 2 — mount the rehearsal session

**Before 15:15.** Attended, TTY-gated, nonce-gated.

```sh
cd adapters/nautilus
R=/ABSOLUTE/path/to/repo
touch "$R/data/rehearsal-daily/rehearsal/keepalive"          # the operator dead-man feeder

export LS_DISPATCH_NONCE=$(date +%s)                          # FRESH, TTL 600 s
LS_TRADING_ENV=paper \
LS_DATA_HOME=$R/data/rehearsal-daily \
LS_REHEARSAL_ENVELOPE=$R/adapters/nautilus/lab/config/rehearsal-envelope.json \
LS_MOUNT_KEEPALIVE=$R/data/rehearsal-daily/rehearsal/keepalive \
LS_REHEARSAL_UNIVERSE_FILE=<the step [10] --out path> \
LS_CALENDAR_SNAPSHOT=$R/adapters/nautilus/state/krx.calendar.json \
LS_DISPATCH_LANE_ENV=$R/.env.domestic \
  ./target/debug/lab-live --rehearse-daily
```

Add `--stop-before-orders` for a **decision-only** session (see Step 3).

**Keep refreshing the keepalive** (`touch`) for the whole session — stale beyond
`heartbeat_interval_secs` (90 s) is an operator dead-man trip.

### The refusal order is the safety property

Paper interlock (66) → attendance/nonce (77) → **every** fail-closed precheck (71) → only then a
node. Every precheck is **pre-build**: a failure leaves **no node built and no order sent**, and the
book unchanged (`live_daily/mount.rs:172-175`). The prechecks, in order:

1. **Envelope** loads and validates (version 1; clock monotone `mount_cutoff < decision <
   auction_end <= session_end`; `stop_grace_secs <= heartbeat_interval_secs`).
2. **Keepalive file exists.**
3. **Trip gate** — a standing `Engage` in `rehearsal/trips.jsonl` refuses the mount.
4. **Mount cutoff** — past 15:15 there is too little of the pre-decision sweep left to establish a
   mark for every mounted symbol, and the marketable-limit policy **refuses to price** an order
   without one. A late mount does not trade badly; it trades nothing while holding a live node open
   against an inherited book.
5. **Calendar** — `LS_CALENDAR_SNAPSHOT` present, a proven session before today, and at least
   `holding_period_sessions` candidate days ahead.
6. **Universe file** matches the session date and is non-empty; **book** loads and `assert_fresh`
   passes.
7. **Advisory lock** over the home's catalog (refuses while an ingest holds it).
8. **The book probe**, three checks in cost order: `validate_fields()` locally → `verify_book_on`
   (t0425 resting orders, then `cts_expcode`-paginated t0424 holdings vs the book's quantities) →
   the **D+2 deposit** last.
9. **Instruments** — every mounted id carries a cached `Equity`, or the limit policy has no daily
   band to clamp to.
10. **Node build** and **manifest build**.

> **The deposit is read from t0424 `sunamt1`, which lags `d2dps` by one session on a day the account
> filled.** It is a pre-mount gate, not a current figure; nothing downstream may re-read it as one
> after the auction.

---

## Step 3 — the decision, the auction, the teardown

The driver runs unattended-proof but **operator-attended**. What it does:

- **15:00–15:20** — sweeps t8407 every 20 s, publishing marks. The sweep is **partial by design**: a
  symbol the gateway omits or quotes unusably is *absent* from the map rather than defaulted.
- **15:20** — the decision read, up to 3 attempts. Held symbols missing from the decision quotes get
  a typed `held_symbol_gaps` row and **keep their position** (KTD12) — a trading halt is not
  evidence a position should be exited; stop and expiry are judged on the next valid bar. This is a
  *divergence class*: the backtest's policy for the same condition is to **abort**.
- If the decision read produces no usable row after its attempts, the session records **"no
  decision"** (holdings kept) and that is **never a trip** (R33).
- **15:20–15:30** — the strategy's market orders are converted by the adapter's marketable-limit
  policy to limits `k = 3` ticks across, clamped to the day's band (buys round **up** the tick grid,
  sells **down**). In a single-price auction every crossing limit fills at the close, so `k` is fill
  insurance, not a price lever.
- **15:33** — stop request, then the fail-closed teardown: stop emission → cancel all resting →
  confirm → engage the kill switch **after** the closing cancels. Then finalize.

### `--stop-before-orders`

The first session's mode. It short-circuits **after** the decision is resolved, published, marked
and its gap rows recorded, and **before** any synthetic bar is delivered — so the strategy emits
**nothing**. It then waits out the session to `session_end_kst`, heartbeating normally.

Use it to observe what t8407's `price` field **means** between 15:20 and 15:30 (the last continuous
trade, or the auction's expected clearing price) without trading on it. The flag does **not** change
the exit code: it is a recorded non-failure, so the run finalizes `0` if the teardown confirmed the
book and there was no hard stop. Note that `closes` stays empty, so every held leg lands in
`stale_basis` and keeps the previous session's day basis — expected, and reported.

### Exit codes — `lab-live --rehearse-daily`

| exit | meaning |
|---|---|
| `0` | **clean** — the session ran, the teardown confirmed the book, and the run finalized |
| `66` | not paper — `LS_TRADING_ENV != paper`. Nothing touched |
| `77` | attendance refused — no TTY, `CI`/`GITHUB_ACTIONS` set, or the nonce is absent/stale (>600 s)/future-skewed (>60 s). Nothing touched |
| `71` | a pre-build precheck refused — **no node was built and no order was sent**, the book is unchanged |
| `72` | **ABNORMAL** — see below. Never success |
| `1` | a staging/finalize failure that left no artifacts at all (not an operator-recoverable refusal) |

**`0` vs `72`.** A session is ABNORMAL on any of three independent causes:

1. **The teardown hard-failed** — on this lane `flat_confirmed` does **not** mean flat. The flatness
   predicate is swapped for the **book** predicate: it means the broker's t0424 matched the intended
   book. An unreadable or truncated t0424 is "book NOT confirmed", and that is abnormal.
2. **Hard stop** — `node.run` did not return within `stop_grace_secs` (60 s) of the stop request. A
   hard stop with a *confirmed* book is still `72`.
3. **The day loop failed or panicked** before the session took its decision. (A cancelled join is
   explicitly not abnormal.)

> **Trap — the `72` message names the wrong variable.** The hard-stop text says
> `LS_MOUNT_STOP_GRACE_SECS`. That variable is the **ladder's** (`live/mount.rs:1545`); this lane
> builds its driver config from the envelope, so setting it changes nothing here. Edit
> `stop_grace_secs` in `config/rehearsal-envelope.json` instead — and mind the envelope's own rule
> that it must stay `<= heartbeat_interval_secs`.

A `72` **de-escalates nothing** — a rehearsal's sessions count toward no rung. But the next session
inherits this account, and a trip in `rehearsal/trips.jsonl` refuses the next mount until cleared.

---

## Step 4 — after the close

The session leaves artifacts in two places.

**The run registry** — `<data_home>/runs/<run_id>/`, staged as `.tmp-<run_id>` and atomically
renamed. A leftover `.tmp-` directory **is** the aborted-run marker.

| artifact | what to read it for |
|---|---|
| `manifest.json` | `rehearsal: true`, `paper_stage: false` — the typed labels (KTD2) |
| `performance.json` | realized P&L and per-trade risk capital |
| `observation.json` | the session row, in `RunObservation` shape (fail-soft: a failure here becomes a data-quality line, never a lost run) |
| `decisions.jsonl` | what the strategy decided, and why |
| `data_quality.json` | `held_symbol_gaps`, `rehearsal_divergences` (`DecisionVsClose`, `UnfilledEntry`), notes, teardown retries, `hard_stopped` |
| `inherited-book.json` | the book this session **inherited**, captured at mount time — the only record of which run opened each leg (see below) |

### Read the session rows

```sh
LS_DATA_HOME=$R/data/rehearsal-daily \
  ./target/debug/lab-research report rehearsal --run <run-id>
```

The run is **never defaulted** — the latest-finalized lookup deliberately partitions
rehearsals out, so a default would resolve a different run than you meant.

It applies `lab/config/transaction-costs.json` at read time and prints **net** session rows.
Note what "net" means here: these are real fills, so slippage is already inside the realized
P&L; what the report adds is the deterministic statutory + brokerage term, which the live
artifact deliberately books at zero. It also prints the halt-day divergence class (KTD12)
separately from the rows, because the backtest **aborts** where the live lane **holds** — and
that comparison is the whole reason the class is typed.

Like `report sample`, it prints net RoR and never a KRW P&L. The KRW figure the runbook asks
you to log is in the run's `observation.json`.

**A session that closed nothing still reports.** That is the normal shape of a halt day, of a
`--stop-before-orders` session, and of an entry still inside its 16-session hold — so it is
the shape of every session before the first exit. You get a "no realized row" line and the
divergence classes below it; a nonzero exit means input or I/O failure, never an empty
session.

Two things the rows will not do, both deliberate:

- **A row containing an exit with no entry-risk join has no net RoR at all** — not a partial
  one. The numerator would hold every trade's P&L while the denominator held only some, which
  inflates the ratio. `dominance_fold` and `RunObservation::build` (R25) refuse the same
  statistic on the same artifact; the row says how many exits were unjoined.
- **A leg carried in from an earlier session is costed on its sell side only.** Its entry is a
  synthetic seed fill, not an execution — no order was sent and no commission was charged — so
  the entry-side cost belongs to the run that opened the leg.

> **Why `inherited-book.json` exists.** `rehearsal/book.json` is live: the teardown rewrites
> it from the broker's snapshot and keeps only what is still held, so the leg an exit closed
> — and its `entered_under` label — is **gone** from it by the time any report runs. The
> mount-time capture inside the run is the only place that label survives. After the holdout
> CLEARs, it is what lets a paper-stage row exclude the exits of rehearsal-entered legs
> (R28); a run written before this artifact existed reports every exit as "opened this
> session" and **says so**, rather than passing the absence off as a check.

A rehearsal writes **no** tracking sidecar and produces no rung evidence — the ladder-only tail is
deliberately skipped.

**The rehearsal state** — `<data_home>/rehearsal/`:

- `book.json` — written by the teardown, **only** when the account was positively confirmed against
  the intended book, tmp + rename so a crash leaves the previous book intact. If it could not be
  confirmed the driver prints a WARNING and does **not** rewrite it; the previous book stands and the
  next mount will refuse on its stamp until the account is reconciled.
- `trips.jsonl` — append-only, one `Engage` row per mechanism plus a `KillSwitch` row per trip.
- `book-adoptions.jsonl` — append-only adoption audit.

Record the session in [`TURN-LOG.md`](TURN-LOG.md): the exit code, the decision, the observed
divergence (15:20 decision price vs the 15:30 close; the 15:20 low vs the whole-day low), the
deposit, and anything the run's `data_quality.json` flagged.

---

## Environment variables

| variable | used by | required | notes |
|---|---|---|---|
| `LS_SM_PROFILE` | morning chain | yes, `daily-rehearsal` | anything but `orb`/`daily-rehearsal` is exit 64 |
| `LS_SM_SESSION_DATE` | morning chain | **effectively yes** | the PREVIOUS session; the default is a stale literal |
| `LS_SM_MOUNT_DATE` | morning chain | yes | today |
| `LS_SM_DATA_HOME` | morning chain | no | must be ABSOLUTE if set; defaults to `data/rehearsal-daily` |
| `LS_SM_INGEST_BY` / `LS_SM_UNIVERSE_BY` | morning chain | no | default 15:00 / 15:10 |
| `LS_TRADING_ENV` | mount, adopt | **yes**, `paper` | mount → 66; `--rehearsal-clear-trip` does **not** read it |
| `LS_DISPATCH_NONCE` | mount, both recovery verbs | **yes** | `date +%s`; TTL 600 s, max future skew 60 s |
| `LS_DATA_HOME` | mount, both recovery verbs | **yes** | the rehearsal home, ABSOLUTE |
| `LS_REHEARSAL_ENVELOPE` | mount | **yes** | `lab/config/rehearsal-envelope.json` |
| `LS_MOUNT_KEEPALIVE` | mount | **yes** | its **mtime** is the operator dead-man feeder |
| `LS_REHEARSAL_UNIVERSE_FILE` | mount; adopt | **yes** for mount | for `adopt`, required **only** when admitting an unknown holding (it supplies the ATR(1)) |
| `LS_CALENDAR_SNAPSHOT` | mount; adopt | **yes** for mount | for `adopt`, required only when admitting |
| `LS_DISPATCH_LANE_ENV` | mount, adopt | no | else `.env.<LS_LANE>`, `LS_LANE` default `domestic` |
| `LS_CALENDAR_ADOPTION` | mount, adopt | no | defaults to `Enforced` |
| `CI`, `GITHUB_ACTIONS` | all gated verbs | must be **unset/empty** | any non-empty value is the unattended marker → 77 |
| `LS_REHEARSAL_STUB_CLOCK` / `LS_DISPATCH_NOW_UNIX` | — | **never in operation** | a test seam: the clock override needs *both*, deliberately, so overriding the cutoff is an act rather than a stale export |

---

## Recovery

### A standing trip refuses the next mount

The mount reports which mechanism stands. Reconcile the account **first**, then:

```sh
export LS_DISPATCH_NONCE=$(date +%s)
LS_DATA_HOME=$R/data/rehearsal-daily \
  ./target/debug/lab-live --rehearsal-clear-trip --why "<what you reconciled, and how>"
```

`--why` is **mandatory** and whitespace-only counts as empty — re-arming after a safety trip must
record who cleared it and why. It writes one `Clear` row per **still-engaged** mechanism, not just
the first: a dead-man `Engage` is not cleared by a later breaker `Clear`, and a verb that released
one while leaving the other standing would send you back to a refused mount.

Exits: `0` cleared · `77` no TTY / bad nonce · `71` everything else (missing `LS_DATA_HOME`, empty
`--why`, **nothing standing**, ledger failure). There is no `66` — this verb has no paper interlock.

> The ladder's `--clear-killswitch` does **not** apply. It writes to a dispatch chain this home does
> not have.

### The book and the account disagree

`--rehearsal-book adopt` cancels every resting order and rewrites `book.json` **from the account**.
The book is never hand-edited.

```sh
export LS_DISPATCH_NONCE=$(date +%s)
LS_TRADING_ENV=paper \
LS_DATA_HOME=$R/data/rehearsal-daily \
LS_DISPATCH_LANE_ENV=$R/.env.domestic \
  ./target/debug/lab-live --rehearsal-book adopt --why "<why>"
```

> ⛔ **Refused 09:00–15:40 KST.** Inside the session window t0424's `price` is a live trade, not a
> close, and every leg's `prior_close` is re-based on it. Run it **before the open or after the
> closing auction** — 15:40 is ten minutes past the 15:30 auction, so the auction's print is what
> t0424 reports. Inside the window **nothing is canceled and nothing is written**. Note the refusal
> is exit **71**, not a code of its own.

`adopt` in order: window check → read the previous book → **cancel all resting** (t0425, fail-closed
on truncation) → confirm the cancel with a second t0425 read → read holdings (`cts_expcode`-cursored
t0424) → rebuild → append the audit rows → tmp+rename the book.

**`--why` becomes mandatory** when the rebuild would lose or invent a fact:

| condition | why `--why` |
|---|---|
| the previous book is unreadable | replacing it re-admits every holding with a *derived* stop |
| a book leg is no longer held | dropping one erases entry-fixed facts nothing else records — and an empty or truncated t0424 read looks exactly like this |
| the account holds a symbol the book does not know | admitting one records a stop the session never set |

An **admitted** leg gets `entry_price` = the broker's average, `stop_price = entry − 1.5 × ATR(1)`
(the frozen `stop_atr_mult`), `entry_date` = today, and a synthetic `entered_under`. It must
therefore be admitted **on a trading day** the calendar proves, and `LS_REHEARSAL_UNIVERSE_FILE`
must supply the ATR — otherwise it refuses.

Exits: `0` written · `1` the sub-verb was not `adopt` · `66` not paper · `77` no TTY / bad nonce ·
`71` everything else. **The cancel pass is not undone** by a later refusal: several `71` messages end
"the resting orders were canceled; the book was not rewritten". An adoption **landed** iff its rows
carry no `Aborted` row for that `adoption_id` — and then `book.json`'s `run_id` equals it.

### Restarting

There is no resume. A session that exited before the teardown wrote no book, so the previous book
stands.

1. Read the exit code and, if a run directory exists, its `data_quality.json`.
2. A `.tmp-<run_id>` directory is an **aborted run**; leave it as the marker.
3. Clear any standing trip (above). Reconcile the account against `book.json`; adopt if they differ
   — remembering the 09:00–15:40 refusal, which means a mid-session repair is not available.
4. Re-mount **only if it is still before 15:15**. Past the cutoff there is nothing to salvage today:
   stand down and record it.

A `71` costs nothing — no node, no order, unchanged book — so re-running after a fix is safe.

---

## Days with changed market hours

KRX moves the session on some days (the CSAT day, and others). The envelope carries **one global
clock and no per-date field**, so there is currently no supported way for it to "name that date's
times".

**Therefore: stand down on any day whose market hours differ from the standard session** (R33). Do
not mount. Record the stand-down in the TURN-LOG.

Editing the envelope's times for a single day is physically possible — it is an operational file —
but it is a global edit affecting every later session, so if it is ever done it must be recorded in
the TURN-LOG and reverted the same day. Prefer the stand-down.

---

## Session cadence

**Attend on the days you can; there is no minimum interval and no requirement to run consecutively.**
The first three sessions in particular need not be consecutive days.

But understand what a skipped session does: **the hold clock does not pause.** A leg's expiry is
counted in *proven trading sessions* from its `entry_date` — the calendar's sessions, not the ones
you attended (KTD11). A session you skip still ages every open hold by one. Consequences to plan for:

- A leg can reach its 16-session expiry on a day nobody is at the terminal. It is not exited then;
  it sits until the next attended session, whose decision will exit it late.
- That lateness is a **divergence from the frozen mechanism**, not a bug. Record it in the TURN-LOG
  with the number of sessions skipped, so the comparison can exclude or annotate it.
- If you intend to skip while holding, prefer skipping from a **flat** book.

---

## The learning contract for pre-judgment sessions

A session run **before** the holdout judgment is not evidence. What it may and may not change:

| may change | may **not** change |
|---|---|
| the driver (`lab/src/runner/live*`, the adapter's live path) | any strategy parameter |
| this runbook | the strategy itself (`daily.rs` and its signal) |
| the envelope's operational values | `config/preregistration.json` |
| the morning script | the judgment home, the frozen catalog, the holdout window |

The reason is head identity: `daily_strategy_code_hash` + `daily_governed_params_hash` are pinned,
and the judgment must rule on the **same** code and parameters the rehearsal exercised. A
parameter change discovered from a pre-judgment session would make the rehearsal and the judgment
two different heads — and a parameter tuned on observed live sessions is a fitted parameter with no
registered effect.

If a rehearsal session suggests the *strategy* is wrong, that is a finding to record, not to act on.
It belongs in the TURN-LOG, and it waits for the judgment.

---

## After the holdout judgment

### If the judgment CLEARs — the paper-stage transition

1. **Declare the divergence ceiling first**, in the TURN-LOG, **before** the first paper-stage
   session: the ratio of (fill price − decision price) to the stop width, above which a session is
   excluded from the comparison (R28). Declaring it afterward would let the data choose the bar.
2. Sessions then open with `rehearsal: false, paper_stage: true`.
3. **Exits of legs whose `entered_under` is a rehearsal run are excluded from paper-stage rows.**
   The leg was opened under a label that produced no evidence; its exit cannot enter a row that does.
4. Rehearsal sessions are **not** reclassified retrospectively. The frozen text speaks of forward
   runs after the judgment, and the plan settles this explicitly (`:205`).

### If the judgment FAILs — driver falsification only

There is no certified head and the ladder path does not open. The rehearsal may continue, but its
purpose narrows to **falsifying the driver**, and it runs under a **session-count cap**.

**The cap is declared by the judgment turn, not by this runbook.** U6's FAIL entry records the number
in the TURN-LOG along with the narrowed purpose (plan `:487`). This runbook's rule:

- No rehearsal session is authorized once the declared cap is reached.
- Raising the cap afterward is a **new pre-registration act**, not an amendment — the same standard
  that governs re-judging a holdout.
- Until the FAIL entry exists, there is no cap and no authorization to rely on one.

---

## What this lane will never do

It never consumes or appends to a dispatch chain, never produces rung evidence, never runs
`--clear-killswitch`, never places a flattening order at teardown (positive confirmation only), and
never runs unattended. `node.run` on this lane is driven by `--rehearse-daily` and nowhere else.
