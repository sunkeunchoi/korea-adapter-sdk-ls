# Concepts

Shared domain vocabulary for this project — entities, named processes, and status concepts with project-specific meaning. Seeded with core domain vocabulary, then accretes as ce-compound and ce-compound-refresh process learnings; direct edits are fine. Glossary only, not a spec or catch-all.

## TR & dispatch

### TR
One LS-securities API operation, identified by a transaction code (e.g. `t1102`, `CSPAQ12200`). The SDK models each TR as a request/response pair plus a runtime endpoint policy, and tracks per-TR metadata (facets, support tier, provisional facets) independently of every other TR. The unit this whole codebase is organized around.
*Avoid:* transaction (generic), endpoint (a TR is more than its path — several TRs can share one path, distinguished by the transaction code).

### owner_class
The dispatch class a TR is routed through, naming which SDK handle exposes it and which runtime machinery it uses: market-session reads, single-page paginated reads, account-state reads, realtime/websocket, order operations, plus a placeholder used before a TR's class is confirmed. A TR's owner_class is corrected from the placeholder to its real class only when the TR becomes callable.

### Facet
A single classified property of a TR (its session scope, instrument domain, rate bucket, pagination behavior, caller-supplied identifiers, and similar). A facet is either hard-accurate (confirmed against the captured spec) or provisional (best-effort, pending live confirmation — tracked in the Provisionality Ledger).

### In-block / Out-block
The named blocks of a TR's wire payload: an in-block carries the caller-supplied request fields, an out-block carries one section of the response. A single TR may expose several out-blocks — commonly a single-record header block alongside a repeated-row array block — and each block's name is the load-bearing key the SDK struct binds to.

## Support lifecycle

A TR climbs a three-rung support ladder; each rung is a deliberate, separately-gated promotion.

### Raw
Below Tracked: the TR's wire shape exists in the captured OpenAPI universe (raw capture, `code-set.json`, migration-source dependency map) but it has no committed `metadata/trs/<tr>.yaml` and no normalized baseline, so it is not yet observed for drift and the `implement-tr` recipe cannot derive structs for it. Bringing a raw TR to Tracked — authoring metadata and pinning a baseline from the raw capture — is a prerequisite step that earlier waves did not face because their members were already Tracked.

### Tracked
The lowest *maintained* rung: the TR has committed metadata and a maintained baseline but no callable code. It is observed for drift, nothing more.

### Implemented
The middle rung: the TR has hand-authored callable Rust and has passed a Paper Live Smoke (a representative call that constructs, returns a success code, yields a non-empty result, and deserializes). An Implemented TR is callable but carries no recommendation and no recorded evidence — explicitly *not* endorsed for production use.

### Recommended
The top rung: an Implemented TR additionally cleared for production use, backed by recorded Focused Evidence and a recommendation block. Promotion to Recommended is a separate, deliberate act beyond Implemented. As of the [[Error-resilience gate]] it additionally means "this call fails gracefully" — promotion requires passing the gate (error coverage), not just a green happy-path smoke.

### Error-resilience gate
The blocking condition, layered on top of the happy-path [[Paper Live Smoke]] + [[Focused Evidence]] + recommendation block, that a TR must pass to reach [[Recommended]]. It redefines the badge to mean "this call fails gracefully": invalid input is caught before the network call ([[Preflight validation]]) and every reachable gateway error is explained. Its per-TR evidence is [[Error coverage]], produced by the [[Differential negative probe]].

### Preflight validation
Rejecting an invalid request *before any network call* with a typed `LsError::Invalid { field, reason }`, from a per-TR [[Constraint schema]]. Runs at the single `dispatch_once` seam (so it covers every owner_class, including orders). Deliberately **permissive by default**: only `type`/`required` (grounded against the normalized baseline) always block; value-class bounds (enum/range/format) stay permissive until a live probe confirms them, because a false-reject silently breaks a caller's valid request with no detector.

### Constraint schema
A declarative per-TR sidecar (`metadata/constraints/<tr>.yaml`) describing each request field's type, required-ness, and per-input-class markers (enum/range/format, each explicitly N/A when inapplicable). The single source from which preflight validation, the negative probe, and the Reference "Errors & validation" section are all derived. Grounded offline against the TR's normalized baseline.

### Differential negative probe
The operator-run probe that certifies a [[Constraint schema]] against paper: a valid control request plus each mechanically-generated invalid variant run in the *same session*, so the injected violation — not an environment condition — is what a rejection reflects. Outcomes per variant: CLEAN (control ok, variant rejected), DIVERGENT (control ok, variant accepted — blocks promotion), or HELD (control failed → inconclusive, distinct from a divergence). Gates re-promotion, never the offline CI gate.

### Error coverage
The per-TR evidence artifact (`metadata/error-coverage/<tr>.yaml`) recording the [[Differential negative probe]]'s per-(field, class) outcomes plus the reachable gateway codes the Reference page explains from the shared error catalog. Required on a Recommended TR by the validator ([[Error-resilience gate]]).

### Paper Live Smoke
A credential-gated integration test that hits the real LS *paper* gateway with real credentials to prove a TR is genuinely callable. It is the gate for flipping a TR to Implemented; a smoke that returns an empty result leaves the TR callable-but-unconfirmed (pending), not Implemented. For `realtime`/websocket TRs the gate is instead *lifecycle (Transport) reachability* — a clean connect → subscribe → unsubscribe — and a row that does or does not arrive is bonus, not the gate; row contents stay provisional until a separate FrameDecode pass.

### Finish-the-flip
A Tracked TR whose callable Rust is **already fully wired** — request/response carrier, `{TR}_POLICY` registered in both crosscheck lists, a `live_smoke_{tr}` and Makefile target — yet still sits at `support.implemented: false` because its certifying [[Paper Live Smoke]] has not yet returned a non-empty in-window result (typically a prior wave staged the build under a closed window or an empty paper feed). For a finish-the-flip TR the remaining work is **metadata + docgen only**, never authoring: re-run the existing smoke, and on a non-empty witness flip `implemented` and bump the docgen counts. Re-authoring one collides on its already-defined symbols, so a flip wave greps for an existing carrier/smoke before treating a candidate as needing an `implement-tr` pass.

### Connection-reachable-only
The calibrated reachability claim a `realtime`/websocket TR carries when its lifecycle smoke passes but the subscribe path is fire-and-forget (the SDK never reads the subscribe ACK) and the gateway emits no observable rejection for an invalid `tr_cd`. A clean connect → subscribe → unsubscribe then proves the *connection* works, not that the specific channel is individually reachable — so the TR is Implemented but its claim is recorded as connection-reachable-only, not per-TR-reachable. Earning the stronger per-TR claim requires the SDK to read the subscribe ACK (a separate capability). Established empirically by the WebSocket negative control.

### WebSocket negative control
The check that calibrates a realtime reachability claim: subscribe a deliberately-invalid `tr_cd` and observe whether the gateway signals a rejection. A `tr_cd`-attributable inbound body (non-empty `rsp_cd`) is `OBSERVABLE` (per-TR reachability is provable); a bare stream close or decode error is `INCONCLUSIVE`; pure silence is `NOT-OBSERVABLE` (flips are [[Connection-reachable-only]]). It has a deterministic mock-WS twin and a live half (`make live-smoke-ws-negative`); its verdict gates how strong a flip's claim may be, not whether the flip happens.

### Closed-window reachable
A curated per-TR judgment that a read's [[Paper Live Smoke]] can certify a non-empty result while KRX is closed, because the read's *shape* does not depend on the live session: a historical-bar pull (a stored time series addressed by symbol + time cursor) or a persistent status board (a standing administrative list that survives across sessions). It is NOT a metadata facet — every candidate carries the same `venue_session: krx_regular` as the session-gated reads — so the call is hand-made and backstopped by the non-empty-assert-before-record gate (a wrong guess dispositions to pending, not a false flip). Contrast: live quotes/order-books/expected-index reads return empty `00707` under closure and must wait for an open window. A non-obvious third case is an account *capacity/deposit computation* read (margin order-quantity, provisional-settlement deposit, balance valuation): it returns a deserializable non-empty row whose every substantive field is zero — a non-flip that does not look empty — unless the account actually carries margin/F-O funding. Only stored cash-summary and reference reads (deposit `sunamt`, a loanable-stock list) are closed-window reachable in the account lane.

### Holdings gate
The pattern that determines whether the paper account holds securities/positions before any positions-dependent read is queued for a flip: model the holdings TR first (e.g. `t0424`) and read its position out-block array *length* from the typed smoke — not the raw-probe `body_len`, which a cash-summary block always inflates. An empty array on a non-default cash summary means cash-only; that single determination *cascades*, downgrading the whole positions/F-O cohort to expected-empty so their all-default smoke results are predicted, not surprising.

### Credential lane
A per-account paper credential set — `(apikey, secret, account_no)` — selected for a request by the TR's `instrument_domain` facet (`stock` → domestic, `futures_options` → domestic-option, `overseas_stock` → overseas, `overseas_futures` → overseas-option). It exists because the LS gateway resolves the target account **entirely from the OAuth token**, never from the account number on the wire: dispatch sends only `tr_cd`/continuation/content-type/bearer-token, and LS's account-read request bodies carry no account field. So one appkey reaches exactly one account, and reaching a different account requires a different appkey, not a different number. A read that returns empty `00707` may simply be authenticating as the wrong account (a wrong-lane artifact) rather than carrying no data — distinct from the genuinely-unfunded case behind the [[Holdings gate]]. A lane whose credential file is absent **fails fast** rather than falling back to another account — a silent fallback would reintroduce the wrong-account hazard (see [[offline-makefile-guard-test-via-real-recipe-shim]] for the guard's regression net).

### Focused Evidence
A recorded, credential-free result of a Paper Live Smoke that backs a Recommended TR's claim. A smoke run gates Implementation; it only becomes Focused Evidence when a TR is deliberately promoted to Recommended.

### Provisionality Ledger
The repository-level sidecar that records, per TR, which authored facets are still provisional and what must be re-verified before promotion. Rows retire as a TR is implemented and each facet is confirmed against a live call; a pending or held TR keeps its rows so nothing is silently treated as confirmed.

### Pending
A TR whose Paper Live Smoke ran but did not open the Implemented gate — callable yet shape-unconfirmed (empty result), or blocked by an unresolved input or an environmental gateway rejection. A pending TR ships without flipping to Implemented and keeps its provisional ledger rows. Distinct from [[Paper-incompatible]]: a Pending TR is expected to flip on a recovering re-run, where a Paper-incompatible TR never flips on paper.

### Paper-incompatible
A TR the paper gateway will never serve, so it is recorded as a permanent non-flip (the `paper_incompatible` facet) rather than a re-runnable [[Pending]]. *Avoid:* paper-unavailable.

Three terminals reach this status, distinguished by the gateway signal: a hard *service-rejection* (the gateway rejects the read outright in any window), an *account-incapable* rejection (the operation needs a paper account the current one is not provisioned for — per-account, not per-service, so it recovers once such an account exists), and an in-window *feed-unprovisioned* empty (a clean success with no data even inside the correct session window — the disambiguating test against an off-window empty, which is a session-clock timing miss and merely [[Pending]]). The `paper_incompatible` facet is a documentation/routing flag meaning "won't flip on paper"; it does **not** imply the runtime paper-incompatible classifier fires — that classifier is bound to the service-rejection code only, and stays silent for the feed-unprovisioned terminal.

## Order safety

The order class is the one place where a bug is a real, irreversible market action rather than a stale read, so it carries its own machinery and vocabulary.

### Double fill
The cardinal order-class failure: the same order placed twice at the exchange. The whole order-safety package (no-retry dispatch, the deduplicator, reconciliation) exists to make a double fill structurally hard. Its asymmetry drives the design — a false "already done" is harmless (reconcile and skip), a false "not done, safe to retry" causes the irreversible second fill — so every order guard fails toward the not-safe conclusion.

### Ambiguous order outcome
An order send whose result cannot be proven Accepted or Rejected: a transport timeout / 5xx, or an order acknowledgement carrying the generic-success code (`00000`/empty) that — unlike a read — does not prove acceptance. Because dispatch is no-retry, an ambiguous outcome is surfaced (as `LsError::AmbiguousOrder` or a transport error) and resolved by [[Order reconciliation]], never blindly retried. Distinct from a clean rejection (a recognized broker reject code → `ApiError`).

### Order reconciliation
The post-ambiguity resolution: query the order/execution read (`t0425`) for the symbol, match candidate rows against the local intent (by order number, else symbol/side/quantity/price), and classify the outcome into one of six states — Accepted, Rejected, Duplicate, Modified, Canceled, Unknown. A retry is authorized only when a *complete* query proves no matching order was accepted; a failed or truncated query, or a degenerate match key, fails toward Unknown + not-safe. Reconciliation is **action-aware**: a submit asks "did a new order appear?", while a modify/cancel references an *existing* order by its original order number and the matcher scans **all** rows for the strongest classification (a `취소`/`정정`/`거부` transition outranks a still-received original). A modify is idempotent-by-target (a rejected modify is safe to re-send); a cancel inverts the risk (see [[Inverted cancel risk]]).

### Inverted cancel risk
The cancel-specific direction of the [[Double fill]] asymmetry: where a submit's danger is placing a second order, a *cancel* wrongly believed to have succeeded leaves a **live resting order** in the market. So [[Order reconciliation]] for a cancel classifies Canceled only on explicit proof (a `취소` row for the referenced order); every other observed state — still-resting, modified, or even a cancel-rejection — fails toward "still-live", never assumed canceled, and never clears the retry guard. At the order class's Implemented flip this path stays mock-proven only (a clean [[Guarded paper order]] run never produces an ambiguous cancel against a genuinely live order).

### Guarded paper order
The order class's Implemented gate — a paper-only evidence matrix (a resting far-from-market buy and sell, one marketable order, one deliberate rejection) that pins the order success predicate from observed real broker codes. It replaces the read class's [[Paper Live Smoke]] (the automated gate never submits a live order) and the realtime class's lifecycle gate. Originally operator-initiated; the chained variant is now **agent-invocable** under a fail-closed autonomy precondition (a CI/no-TTY refusal plus a fresh per-wave human nonce) — autonomy removes the operator-handoff *protocol*, not the human, and pairs with an [[Account-flat assertion]] as its post-run safety net. If the paper account cannot place in-window, the run records [[Pending]] and the order TRs do not flip.

### Account-flat assertion
The post-run safety net of an autonomous [[Guarded paper order]] run: after the submit→modify→cancel teardown, a `t0425` working-orders scan must positively confirm zero live rows. Because no operator remains to clean up, it keys on quantities not status text — a fill (`cheqty>0`) is unrecoverable (hard-fail, paper reset), a resting remainder (`ordrem>0`) is retry-canceled then hard-failed if it persists, and a cancel-rejected (`취소거부`) order is treated as still-resting despite its 취소 marker. Positive-confirmation-only: a failed/timed-out/**truncated** read is NOT flat — it fails closed on a pagination-continuation signal rather than trust a partial page. Originally an account-wide all-states scan (to catch a prior-run leftover on any symbol); on a heavily-used paper account that exhaustive walk overran the gateway's `t0425` page cap and never completed, so it is now **scoped to the traded symbol and to working (unfilled) orders only**, read as a single page. That scope is sound because the chain trades exactly one symbol at non-marketable prices that cannot fill — it still catches a still-resting order and a partial fill (both carry unfilled remainder) and only forgoes detecting a *fully*-filled or other-symbol row, which the chain cannot create. An [[Ambiguous order outcome]] on the submit leg routes through it rather than recording a silent [[Pending]]. Bounding it required pacing around an LS `t0425` rate-throttle (`IGW00201`) whose per-TR 2/s cap is tighter than the SDK's market-data rate bucket (see docs/solutions/integration-issues/ls-gateway-t0425-rate-limit-and-pagination-flat-scan.md). The form above assumes an Implemented working-orders (미체결) read; a venue lacking one (the domestic **F/O** chain) cannot scan the board, so its flat assertion is **two-part**: a `t0441` balance read detects a *filled* position only (잔고; empty/unreadable = NOT flat, and a short/negative position counts as a fill via `f64 != 0`), and a **clean cancel response** is the only confirmation the resting order was removed. With no board read to verify removal, ANY teardown-uncertain state — an accepted submit returning no order number, an ambiguous modify, or a non-clean cancel — hard-fails rather than concluding flat (see docs/solutions/conventions/authoring-fo-order-tr-chain.md §5).
