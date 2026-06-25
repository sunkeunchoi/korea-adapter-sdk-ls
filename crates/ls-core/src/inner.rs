//! Shared runtime state — the single auth/transport core.
//!
//! `Inner` owns one pooled `reqwest::Client`, the resolved config, an
//! `Arc<TokenManager>` (so a future WebSocket manager can share it without a
//! circular `Arc`), and an `Arc<RateLimiterManager>`. It exposes:
//!
//! - `dispatch_once` (private) — one HTTP attempt, no retry, no rate limit; the
//!   SINGLE place a bearer token and the LS continuation headers are applied.
//! - `post` (public) — retry + rate limit for non-order REST.
//! - `post_paginated` (public) — like `post`, threading `tr_cont`/`tr_cont_key`.
//! - `collect_all` (public) — drives multi-page collection via `HasPagination`.
//!
//! No order-dispatch path exists here.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use backon::{ExponentialBuilder, Retryable};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::auth::TokenManager;
use crate::config_resolve::ResolvedConfig;
use crate::endpoint_policy::EndpointPolicy;
use crate::order_dedup::OrderDeduplicator;
use crate::pagination::HasPagination;
use crate::rate_limiter::RateLimiterManager;
use crate::{LsConfig, LsError, LsResult};

/// The single LS Paper "unsupported work" signal.
///
/// `01900` (모의투자에서는 해당업무가 제공되지 않습니다 — "this service is not
/// provided in Paper") is the ONLY paper-incompatible response code. It is
/// surfaced as a normal `LsError::ApiError { code: "01900", .. }` — the code is
/// preserved verbatim, never collapsed into a generic failure — and callers
/// recognize it via [`is_paper_incompatible`] / [`LsError::is_paper_incompatible`].
pub const PAPER_INCOMPATIBLE_CODE: &str = "01900";

/// Returns `true` if `code` is the sole paper-incompatible signal (`01900`).
///
/// Mirrors the certification harness's `classify_api_error` `01900 =>
/// paper_incompatible` branch, but located in the runtime so any consumer can
/// classify an `ApiError` without a test-only dependency.
pub fn is_paper_incompatible(code: &str) -> bool {
    code == PAPER_INCOMPATIBLE_CODE
}

/// Classify an LS `rsp_cd` response code as success.
///
/// `"00000"` (and an absent/empty code) is the universal success code for query
/// TRs. Read TRs may also return informational completion codes — `00136`
/// (조회가 완료되었습니다, inquiry completed) and `00707` (조회할 내역이 없습니다,
/// inquiry completed with an empty result set). These are successes: the gateway
/// processed the query and the response carries valid (possibly empty) data
/// blocks. Rejections and gateway errors are not in the set and surface as
/// `LsError::ApiError`.
fn rsp_cd_is_success(code: &str) -> bool {
    if code.is_empty() || code == "00000" {
        return true;
    }
    matches!(code, "00136" | "00707")
}

/// Order acknowledgement classification — distinct from the read predicate.
///
/// Orders MUST NOT reuse [`rsp_cd_is_success`]: the read predicate trusts
/// `00000`/empty as success, but for an order those are the gateway's
/// *generic* success codes and cannot prove the exchange accepted the order.
/// See [`classify_order_rsp_cd`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OrderAck {
    /// A recognized order acknowledgement code (`00039`/`00040`).
    Accepted,
    /// A recognized rejection — surfaced as `LsError::ApiError`, code/message
    /// preserved.
    Rejected,
    /// Neither provably accepted nor safely rejected — fail toward Unknown and
    /// route to reconciliation rather than resubmitting.
    Ambiguous,
}

/// The order success seed set (`00039` sell-ack / `00040` buy-ack), recorded in
/// `docs/design/order-safety-design.md` §1. This is the mock-gate baseline; the
/// guarded live evidence run confirms or amends it (a widened live set forces a
/// mock-gate re-run before any flip — KTD2/KTD4).
fn rsp_cd_is_order_success(code: &str) -> bool {
    matches!(code, "00039" | "00040")
}

/// Classify an order `rsp_cd` into [`OrderAck`].
///
/// - `00039`/`00040` → `Accepted` (the seed set).
/// - `00000`/empty → `Ambiguous`. These are the read path's generic-success
///   codes; an order that came back `00000` may well have landed at the venue,
///   so it must never be treated as a rejection (resubmitting risks a double
///   fill). Fail toward Unknown → reconciliation.
/// - anything else → `Rejected` (preserve the broker code/message). This is why
///   `00136`/`00707`, which the read predicate accepts, are *not* order-success.
fn classify_order_rsp_cd(code: &str) -> OrderAck {
    if rsp_cd_is_order_success(code) {
        OrderAck::Accepted
    } else if code.is_empty() || code == "00000" {
        OrderAck::Ambiguous
    } else {
        OrderAck::Rejected
    }
}

/// Return `true` if the error should trigger a retry.
///
/// - `LsError::Http` with a 5xx status → true (server error)
/// - `LsError::Http` with no status (transport error) → true
/// - `LsError::Http` with a 4xx status → false (client error)
/// - all other variants → false
fn is_retryable(err: &LsError) -> bool {
    match err {
        LsError::Http(re) => re.status().map(|s| s.is_server_error()).unwrap_or(true),
        _ => false,
    }
}

/// Shared runtime state, wrapped in `Arc<Inner>`.
pub struct Inner {
    /// Connection-pooled HTTP client, shared across every request.
    pub client: Client,
    /// Immutable resolved runtime configuration.
    pub config: ResolvedConfig,
    /// Bearer-token cache + refresh logic. `Arc` so it can be shared without an
    /// `Arc<Inner>` (avoids a circular `Arc`).
    pub token_manager: Arc<TokenManager>,
    /// Per-category rate limiter. Charged inside the retry closure.
    pub rate_limiter: Arc<RateLimiterManager>,
    /// Global order kill switch (order-safety contract §1). `true` = orders may
    /// dispatch; `false` = the operator emergency halt is engaged and every
    /// `post_order` call halts before dedup, rate limiting, or HTTP I/O.
    /// Non-order dispatch (`post`/`post_paginated`) never consults it. Interior
    /// mutability so `set_orders_enabled` works through a shared `Arc<Inner>`.
    pub orders_enabled: AtomicBool,
    /// Per-client order deduplication cache (order-safety §2). Consulted by
    /// `post_order` after the kill switch, before rate limiting.
    pub order_dedup: OrderDeduplicator,
}

impl Inner {
    /// Construct an `Arc<Inner>` from a validated `LsConfig`.
    ///
    /// Synchronous — no network I/O. The OAuth2 token is fetched lazily on the
    /// first dispatch, so this is callable outside a Tokio runtime.
    pub fn new(config: LsConfig) -> Result<Arc<Self>, LsError> {
        let resolved = ResolvedConfig::from_raw(&config)?;

        // Chain connect_timeout + timeout onto the shared client:
        // - connect_timeout guards the TCP handshake phase only.
        // - timeout guards the full request lifecycle (connect + send + read).
        // Both `fetch_token` and `revoke_token_http` receive this client, so the
        // timeouts apply to the OAuth endpoints too.
        let client = Client::builder()
            .connect_timeout(resolved.connect_timeout)
            .timeout(resolved.request_timeout)
            .build()
            .map_err(LsError::Http)?;
        let token_manager = Arc::new(TokenManager::new());
        let rate_limiter = Arc::new(RateLimiterManager::new(&resolved.rate_limits)?);
        Ok(Arc::new(Inner {
            client,
            config: resolved,
            token_manager,
            rate_limiter,
            // Orders dispatch is enabled by default; the operator engages the
            // kill switch explicitly via `set_orders_enabled(false)`.
            orders_enabled: AtomicBool::new(true),
            order_dedup: OrderDeduplicator::with_default_ttl(),
        }))
    }

    /// Engage or release the global order kill switch (order-safety §1).
    ///
    /// `set_orders_enabled(false)` is the operator emergency halt: every
    /// subsequent `post_order` halts before dedup, rate limiting, or HTTP I/O.
    /// Reconciliation reads the switch but MUST NOT re-enable it. Non-order
    /// dispatch is unaffected.
    pub fn set_orders_enabled(&self, enabled: bool) {
        self.orders_enabled.store(enabled, Ordering::SeqCst);
    }

    /// `true` if order dispatch is currently enabled (the default).
    pub fn orders_enabled(&self) -> bool {
        self.orders_enabled.load(Ordering::SeqCst)
    }

    /// Single HTTP attempt — no retry, no rate limit. The SINGLE auth/transport
    /// enforcement point: every authenticated request flows through here.
    ///
    /// `tr_code` is injected as the `tr_cd` header when non-empty. The LS gateway
    /// requires `tr_cont`/`tr_cont_key` on every request; the first-page defaults
    /// are `"N"` and `""` (omitting them causes HTTP 500 from the paper
    /// gateway). Response `tr_cont`/`tr_cont_key` headers are read BEFORE the body
    /// is consumed and injected into the deserialized JSON so `HasPagination`
    /// getters on `Res` can read them for `collect_all` continuation.
    #[tracing::instrument(skip_all, fields(tr_code, path, http_status, rsp_cd, latency_ms))]
    async fn dispatch_once<Req, Res>(
        &self,
        policy: &EndpointPolicy,
        req: &Req,
        tr_cont: Option<&str>,
        tr_cont_key: Option<&str>,
    ) -> LsResult<Res>
    where
        Req: Serialize,
        Res: DeserializeOwned,
    {
        let span = tracing::Span::current();
        span.record("tr_code", policy.tr_code);
        span.record("path", policy.path);

        // Step 1: obtain a valid bearer token (fetches or refreshes lazily).
        let token = self
            .token_manager
            .get_or_refresh(&self.client, &self.config, &self.rate_limiter)
            .await?;

        // Step 2: assemble URL from resolved config. `base_url` is the single
        // test-injection choke point.
        let url = format!("{}{}", &self.config.base_url, policy.path);

        // Step 3: build request with the Bearer header and continuation headers.
        let mut builder = self.client.post(url).bearer_auth(&token);
        if !policy.tr_code.is_empty() {
            builder = builder.header("tr_cd", policy.tr_code);
        }
        builder = builder.header("tr_cont", tr_cont.unwrap_or("N"));
        builder = builder.header("tr_cont_key", tr_cont_key.unwrap_or(""));

        // Step 4: dispatch with an explicit body + charset=utf-8 content-type
        // (required by the LS spec).
        let start = std::time::Instant::now();
        let body_bytes = serde_json::to_vec(req).map_err(LsError::Decode)?;
        let resp = builder
            .header("content-type", "application/json; charset=utf-8")
            .body(body_bytes)
            .send()
            .await
            .map_err(|e| {
                tracing::debug!(error = %e, "dispatch_once: HTTP send failed");
                LsError::Http(e)
            })?;
        let latency = start.elapsed();
        span.record("latency_ms", latency.as_millis() as i64);
        span.record("http_status", resp.status().as_u16() as i64);

        // Read tr_cont/tr_cont_key response headers BEFORE consuming the body.
        let resp_tr_cont = resp
            .headers()
            .get("tr_cont")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let resp_tr_cont_key = resp
            .headers()
            .get("tr_cont_key")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let status = resp.status();

        if !status.is_success() {
            // LS returns JSON error bodies even on HTTP 5xx. Try to extract
            // rsp_cd/rsp_msg so callers get a structured error instead of an
            // opaque Http.
            let e = resp.error_for_status_ref().unwrap_err();
            let body_text = resp.text().await.unwrap_or_default();
            let envelope = serde_json::from_str::<serde_json::Value>(&body_text).ok();
            let code = envelope
                .as_ref()
                .and_then(|v| v.get("rsp_cd"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let message = envelope
                .as_ref()
                .and_then(|v| v.get("rsp_msg"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // ORDER PATH (second classification site): a non-2xx HTTP status on
            // an order is *ambiguous* regardless of the body — the order may or
            // may not have reached the exchange. Never collapse it to a clean
            // rejection; fail toward Unknown so the caller reconciles via
            // `t0425` rather than blindly resubmitting (order-safety §1/§3).
            if policy.is_order {
                if !code.is_empty() {
                    span.record("rsp_cd", code);
                }
                tracing::debug!(
                    status = %status,
                    rsp_cd = %code,
                    "dispatch_once: ambiguous order outcome on non-2xx status"
                );
                return Err(LsError::AmbiguousOrder {
                    code: code.to_string(),
                    message,
                });
            }

            if !code.is_empty() {
                tracing::debug!(
                    status = %status,
                    rsp_cd = %code,
                    rsp_msg = %message,
                    "dispatch_once: API business error via non-2xx status"
                );
                return Err(LsError::ApiError {
                    code: code.to_string(),
                    message,
                });
            }
            tracing::debug!(
                status = %status,
                error = %e,
                body = %body_text,
                "dispatch_once: HTTP error response"
            );
            return Err(LsError::Http(e));
        }

        // Step 5: decode body, inject pagination state for HasPagination getters.
        let mut val: serde_json::Value = resp.json().await.map_err(|e| {
            tracing::debug!(error = %e, "dispatch_once: body decode failed");
            LsError::Http(e)
        })?;
        // Inject tr_cont/tr_cont_key into the response JSON so Res::HasPagination
        // getters can read them. Guard against non-object responses.
        if let serde_json::Value::Object(ref mut map) = val {
            map.insert("tr_cont".into(), serde_json::json!(resp_tr_cont));
            map.insert("tr_cont_key".into(), serde_json::json!(resp_tr_cont_key));
        }
        // Inspect rsp_cd before deserializing into Res. The classification is
        // policy-dependent: order endpoints use the order predicate (the read
        // predicate trusts `00000`/empty, which an order must not), non-order
        // endpoints use the read predicate.
        if policy.is_order {
            // ORDER PATH (first classification site).
            let code = val.get("rsp_cd").and_then(|v| v.as_str()).unwrap_or("");
            span.record("rsp_cd", code);
            let message = || {
                val.get("rsp_msg")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            match classify_order_rsp_cd(code) {
                // Accepted — fall through to deserialize into Res. The test
                // asserts this is NOT an ApiError, proving the predicate ran.
                OrderAck::Accepted => {}
                OrderAck::Rejected => {
                    tracing::debug!(rsp_cd = %code, "dispatch_once: order rejected");
                    return Err(LsError::ApiError {
                        code: code.to_string(),
                        message: message(),
                    });
                }
                OrderAck::Ambiguous => {
                    tracing::debug!(
                        rsp_cd = %code,
                        "dispatch_once: ambiguous order acknowledgement code -> reconcile"
                    );
                    return Err(LsError::AmbiguousOrder {
                        code: code.to_string(),
                        message: message(),
                    });
                }
            }
        } else if let Some(code) = val.get("rsp_cd").and_then(|v| v.as_str()) {
            // NON-ORDER PATH. Missing/empty rsp_cd is treated as success.
            // `01900` (paper-incompatible) is NOT collapsed — it surfaces as
            // ApiError with its exact code preserved.
            span.record("rsp_cd", code);
            if !rsp_cd_is_success(code) {
                let message = val
                    .get("rsp_msg")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                tracing::debug!(rsp_cd = %code, "dispatch_once: API business error");
                return Err(LsError::ApiError {
                    code: code.to_string(),
                    message,
                });
            }
        }
        serde_json::from_value::<Res>(val).map_err(LsError::Decode)
    }

    /// Record the common span fields shared by `post` and `post_paginated`.
    fn record_request_span(span: &tracing::Span, policy: &EndpointPolicy) {
        span.record("tr_code", policy.tr_code);
        span.record("path", policy.path);
        span.record("category", policy.category.as_str());
        if let Some(v) = policy.rate_limit_per_sec {
            span.record("tr_rate_limit_per_sec", v as i64);
        }
    }

    /// Shared retry pipeline for non-order POSTs.
    ///
    /// The rate-limiter `wait` is INSIDE the retry closure, so each of the
    /// up-to-4 attempts independently charges the bucket. `backon`'s default
    /// `ExponentialBuilder` does up to 3 retries → ≤4 total HTTP calls.
    /// `.when(is_retryable)` retries only 5xx + transport errors.
    async fn post_with_retry<Req, Res>(
        &self,
        policy: &EndpointPolicy,
        req: &Req,
        tr_cont: Option<&str>,
        tr_cont_key: Option<&str>,
    ) -> LsResult<Res>
    where
        Req: Serialize + Sync,
        Res: DeserializeOwned + Send,
    {
        let attempt = std::sync::atomic::AtomicU64::new(0);
        let dispatch = || async {
            let a = attempt.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            tracing::Span::current().record("retry_attempt", a);
            self.rate_limiter.wait(policy.category).await;
            self.dispatch_once::<Req, Res>(policy, req, tr_cont, tr_cont_key)
                .await
        };
        let result = dispatch
            .retry(ExponentialBuilder::default())
            .sleep(tokio::time::sleep)
            .when(is_retryable)
            .await;
        let final_attempt = attempt.load(std::sync::atomic::Ordering::Relaxed);
        if final_attempt > 1 {
            tracing::info!(
                retry_attempt = final_attempt,
                "request succeeded after retries"
            );
        }
        result
    }

    /// Non-order POST with retry + rate limiting.
    ///
    /// The rate limiter waits INSIDE the retry closure, so each of the up-to-4
    /// attempts independently charges the bucket. `.when(is_retryable)` retries
    /// 5xx + transport errors only.
    #[tracing::instrument(
        skip_all,
        fields(tr_code, path, category, retry_attempt, tr_rate_limit_per_sec)
    )]
    pub async fn post<Req, Res>(&self, policy: &EndpointPolicy, req: &Req) -> LsResult<Res>
    where
        Req: Serialize + Sync,
        Res: DeserializeOwned + Send,
    {
        policy.guard_non_order()?;
        Self::record_request_span(&tracing::Span::current(), policy);
        self.post_with_retry::<Req, Res>(policy, req, None, None)
            .await
    }

    /// Paginated non-order POST — retry + rate limit with `tr_cont`/`tr_cont_key`
    /// header propagation.
    ///
    /// Callers (typically `collect_all`) set `req.tr_cont`/`req.tr_cont_key`
    /// before each page call; this reads them via `HasPagination` and passes them
    /// to `dispatch_once` for injection as HTTP request headers.
    #[tracing::instrument(
        skip_all,
        fields(tr_code, path, category, retry_attempt, tr_rate_limit_per_sec)
    )]
    pub async fn post_paginated<Req, Res>(
        &self,
        policy: &EndpointPolicy,
        req: &Req,
    ) -> LsResult<Res>
    where
        Req: HasPagination + Serialize + Sync,
        Res: DeserializeOwned + Send,
    {
        policy.guard_non_order()?;
        Self::record_request_span(&tracing::Span::current(), policy);
        let tr_cont = req.tr_cont();
        let tr_cont_key = req.tr_cont_key();
        self.post_with_retry::<Req, Res>(
            policy,
            req,
            if tr_cont.is_empty() {
                None
            } else {
                Some(tr_cont)
            },
            if tr_cont_key.is_empty() {
                None
            } else {
                Some(tr_cont_key)
            },
        )
        .await
    }

    /// Order POST — the no-retry order dispatch path (order-safety contract §1).
    ///
    /// Unlike [`post`], this issues **exactly one** HTTP attempt: there is no
    /// `backon` retry loop, because a transport timeout / 5xx on an order is
    /// ambiguous (the exchange may or may not have filled), and a blind retry
    /// risks a double fill. An ambiguous outcome surfaces as
    /// [`LsError::AmbiguousOrder`] (or [`LsError::Http`] on transport failure)
    /// for the caller to reconcile via `t0425`, never retried here.
    ///
    /// Order classification runs *inside* `dispatch_once` (the order predicate,
    /// keyed on `policy.is_order`), so a `00039`/`00040` ack deserializes into
    /// `Res` while `00000`/empty fails toward Unknown.
    ///
    /// `instrument(skip_all)` keeps credentials and the request body out of the
    /// span; only the four structural fields are recorded (order-safety §5).
    ///
    /// [`post`]: Inner::post
    #[tracing::instrument(skip_all, fields(tr_code, path, category, dedup_hit))]
    pub async fn post_order<Req, Res>(&self, policy: &EndpointPolicy, req: &Req) -> LsResult<Res>
    where
        Req: Serialize + Sync,
        Res: DeserializeOwned + Send + Serialize,
    {
        // Kill switch FIRST — the operator emergency halt stops all order
        // dispatch before dedup, rate limiting, or any I/O (order-safety §1).
        if !self.orders_enabled() {
            return Err(LsError::ApiError {
                code: "orders-disabled".into(),
                message: format!(
                    "order dispatch is halted by the kill switch; order '{}' was not sent",
                    policy.tr_code
                ),
            });
        }
        // Defense-in-depth: reject a non-order policy before any HTTP I/O.
        policy.guard_order()?;
        let span = tracing::Span::current();
        span.record("tr_code", policy.tr_code);
        span.record("path", policy.path);
        span.record("category", policy.category.as_str());

        // Dedup AFTER the kill switch, BEFORE rate limiting (order-safety §2).
        // The key embeds account_no and is fail-closed: a serialization failure
        // dispatches nothing. The key itself is NEVER logged or traced — only
        // the dedup_hit boolean is observable.
        let dedup_key =
            OrderDeduplicator::key(&self.config.account_no, policy.tr_code, req)?;
        if let Some(cached) = self.order_dedup.get(&dedup_key) {
            // Cache hit: return the cached response, bypassing rate limiting and
            // HTTP entirely. Reconciliation reads dedup_hit to classify Duplicate.
            span.record("dedup_hit", true);
            return serde_json::from_value::<Res>(cached).map_err(LsError::Decode);
        }
        span.record("dedup_hit", false);

        // Charge the Orders bucket exactly once — there is no retry loop to
        // re-charge it.
        self.rate_limiter.wait(policy.category).await;
        let res: Res = self
            .dispatch_once::<Req, Res>(policy, req, None, None)
            .await?;
        // Cache only a successful (Accepted) response. Rejections and ambiguous
        // outcomes are NOT cached — a corrected resubmission must reach the
        // exchange, and an ambiguous send is resolved by reconciliation, not the
        // dedup window. Round-tripping Res through JSON is lossless for its own
        // fields, which is all the caller observes.
        if let Ok(value) = serde_json::to_value(&res) {
            self.order_dedup.insert(dedup_key, value);
        }
        Ok(res)
    }

    /// Collect all pages of a paginated TR by looping until the `tr_cont`
    /// response header is empty/`"N"` or `max_pages` is reached.
    ///
    /// `dispatch_once` injects the response `tr_cont`/`tr_cont_key` headers into
    /// the deserialized JSON so `Res::HasPagination` getters can read them; the
    /// continuation is copied onto a cloned next request via the closure. Returns
    /// `LsError::PaginationLimit(max_pages)` if the loop is still continuing at
    /// the cap.
    pub async fn collect_all<Req, Res, F, Fut>(&self, mut req: Req, f: F) -> LsResult<Vec<Res>>
    where
        Req: HasPagination + Clone + Send + Serialize,
        Res: HasPagination + DeserializeOwned + Send,
        F: Fn(Req) -> Fut,
        Fut: std::future::Future<Output = LsResult<Res>> + Send,
    {
        let max = self.config.max_pages;
        let mut results = Vec::new();
        let mut truncated = false;
        for _ in 0..max {
            // `req.clone()` is required: the closure takes `Req` by value, and
            // the request must survive the call so continuation fields can be set
            // for the next page.
            let page = f(req.clone()).await?;
            // Terminal page: an empty (or "N") tr_cont stops the loop. Test on a
            // borrow first so single-page calls allocate nothing.
            let cont = page.tr_cont();
            if cont.is_empty() || cont == "N" {
                results.push(page);
                truncated = false;
                break;
            }
            // Continuing: extract continuation strings before `push` moves `page`.
            let tr_cont = page.tr_cont().to_string();
            let tr_cont_key = page.tr_cont_key().to_string();
            results.push(page);
            truncated = true;
            req.set_tr_cont(tr_cont);
            req.set_tr_cont_key(tr_cont_key);
        }
        if truncated {
            return Err(LsError::PaginationLimit(max));
        }
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endpoint_policy::{EndpointPolicy, Protocol};
    use crate::{LsConfig, RateLimitCategory};
    use serde::Deserialize;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

    fn mock_config(base_url: &str) -> LsConfig {
        LsConfig {
            appkey: "test-appkey".into(),
            appsecretkey: "test-appsecretkey".into(),
            account_no: "00000000-01".into(),
            environment: crate::Environment::Paper,
            rate_limits: None,
            base_url: Some(base_url.to_string()),
            ws_base_url: None,
            max_pages: None,
            connect_timeout_secs: None,
            request_timeout_secs: None,
            ws_connect_timeout_secs: None,
            allow_insecure_localhost: true,
            ws_channel_capacity: None,
            ws_overflow_policy: None,
        }
    }

    fn mock_config_max_pages(base_url: &str, max_pages: usize) -> LsConfig {
        LsConfig {
            max_pages: Some(max_pages),
            ..mock_config(base_url)
        }
    }

    fn test_policy() -> EndpointPolicy {
        EndpointPolicy {
            tr_code: "TEST",
            path: "/test/path",
            module: "test",
            group: "test",
            protocol: Protocol::Rest,
            category: RateLimitCategory::MarketData,
            is_order: false,
            has_pagination: false,
            rate_limit_per_sec: None,
            corp_rate_limit_per_sec: None,
        }
    }

    /// Mount a wiremock token endpoint that always issues a fixed token.
    async fn mount_token(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "test-token",
                "token_type": "Bearer",
                "expire_in": 86400
            })))
            .mount(server)
            .await;
    }

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct EchoRes {
        rsp_cd: String,
        #[serde(default)]
        rsp_msg: String,
        #[serde(default)]
        value: String,
    }

    #[tokio::test]
    async fn post_happy_path_sends_headers_and_deserializes() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        // The dispatch must carry tr_cd=TEST and the default tr_cont="N".
        Mock::given(method("POST"))
            .and(path("/test/path"))
            .and(header("tr_cd", "TEST"))
            .and(header("tr_cont", "N"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rsp_cd": "00000",
                "rsp_msg": "ok",
                "value": "hello"
            })))
            .mount(&server)
            .await;

        let inner = Inner::new(mock_config(&server.uri())).expect("valid config");
        let res: EchoRes = inner
            .post(&test_policy(), &serde_json::json!({"q": 1}))
            .await
            .expect("post should succeed");
        assert_eq!(res.value, "hello");
        assert_eq!(res.rsp_cd, "00000");
    }

    #[tokio::test]
    async fn non_success_rsp_cd_on_2xx_surfaces_api_error() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path("/test/path"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rsp_cd": "IGW40013",
                "rsp_msg": "데이터 조회 실패"
            })))
            .mount(&server)
            .await;

        let inner = Inner::new(mock_config(&server.uri())).expect("valid config");
        let err = inner
            .post::<_, EchoRes>(&test_policy(), &serde_json::json!({}))
            .await
            .expect_err("non-success rsp_cd must be an error");
        match err {
            LsError::ApiError { code, message } => {
                assert_eq!(code, "IGW40013");
                assert_eq!(message, "데이터 조회 실패");
            }
            other => panic!("expected ApiError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn code_01900_classifies_as_paper_incompatible() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path("/test/path"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rsp_cd": "01900",
                "rsp_msg": "모의투자에서는 해당업무가 제공되지 않습니다."
            })))
            .mount(&server)
            .await;

        let inner = Inner::new(mock_config(&server.uri())).expect("valid config");
        let err = inner
            .post::<_, EchoRes>(&test_policy(), &serde_json::json!({}))
            .await
            .expect_err("01900 must be an error");
        // The mechanism: the exact code is preserved (not collapsed) and the
        // runtime helper classifies it specifically as paper-incompatible.
        match &err {
            LsError::ApiError { code, .. } => {
                assert_eq!(code, "01900");
                assert!(is_paper_incompatible(code));
            }
            other => panic!("expected ApiError, got {other:?}"),
        }
        assert!(
            err.is_paper_incompatible(),
            "LsError::is_paper_incompatible() must be true for 01900"
        );
        // A different non-success code must NOT classify as paper-incompatible.
        assert!(!is_paper_incompatible("IGW40013"));
    }

    /// rsp_cd classification is load-bearing on BOTH 2xx and non-2xx. A non-2xx
    /// status carrying a JSON error envelope must surface the structured
    /// `rsp_cd` as an `ApiError` (not an opaque `Http`), so `01900` arriving on
    /// a 5xx still classifies specifically as paper-incompatible.
    #[tokio::test]
    async fn code_01900_on_non_2xx_classifies_as_paper_incompatible() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path("/test/path"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "rsp_cd": "01900",
                "rsp_msg": "모의투자에서는 해당업무가 제공되지 않습니다."
            })))
            .mount(&server)
            .await;

        let inner = Inner::new(mock_config(&server.uri())).expect("valid config");
        let err = inner
            .post::<_, EchoRes>(&test_policy(), &serde_json::json!({}))
            .await
            .expect_err("01900 on a non-2xx must be an error");
        match &err {
            LsError::ApiError { code, .. } => {
                assert_eq!(
                    code, "01900",
                    "non-2xx rsp_cd must be extracted as ApiError"
                );
                assert!(err.is_paper_incompatible());
            }
            other => panic!("expected ApiError from non-2xx body, got {other:?}"),
        }
    }

    /// Counts hits and returns 503 for the first `fail_first` requests, then 200.
    struct FlakyResponder {
        hits: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        fail_first: usize,
    }

    impl Respond for FlakyResponder {
        fn respond(&self, _req: &Request) -> ResponseTemplate {
            let n = self.hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if n < self.fail_first {
                ResponseTemplate::new(503)
            } else {
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "rsp_cd": "00000",
                    "rsp_msg": "ok",
                    "value": "recovered"
                }))
            }
        }
    }

    #[tokio::test]
    async fn retryable_transport_error_retries_up_to_cap_and_charges_bucket() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        // 503 forever → the retry budget (≤4 attempts) is exhausted.
        Mock::given(method("POST"))
            .and(path("/test/path"))
            .respond_with(FlakyResponder {
                hits: hits.clone(),
                fail_first: usize::MAX,
            })
            .mount(&server)
            .await;

        let inner = Inner::new(mock_config(&server.uri())).expect("valid config");
        let err = inner
            .post::<_, EchoRes>(&test_policy(), &serde_json::json!({}))
            .await
            .expect_err("persistent 503 must fail");
        assert!(
            matches!(err, LsError::Http(_)),
            "expected Http, got {err:?}"
        );
        // ≤4 total attempts (1 initial + up to 3 retries). Each attempt charged
        // the bucket (the wait is inside the retry closure).
        let total = hits.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(total, 4, "expected exactly 4 attempts, got {total}");
    }

    #[tokio::test]
    async fn non_retryable_error_does_not_retry() {
        // 400 is a client error → non-retryable.
        let server = MockServer::start().await;
        mount_token(&server).await;
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let hits_inner = hits.clone();
        Mock::given(method("POST"))
            .and(path("/test/path"))
            .respond_with(move |_req: &Request| {
                hits_inner.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                ResponseTemplate::new(400)
            })
            .mount(&server)
            .await;

        let inner = Inner::new(mock_config(&server.uri())).expect("valid config");
        let err = inner
            .post::<_, EchoRes>(&test_policy(), &serde_json::json!({}))
            .await
            .expect_err("400 must fail");
        assert!(
            matches!(err, LsError::Http(_)),
            "expected Http, got {err:?}"
        );
        let total = hits.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            total, 1,
            "non-retryable error must not retry, got {total} attempts"
        );
    }

    // --- Pagination: a local paginated request/response pair ----------------

    #[derive(Debug, Clone, Serialize)]
    struct PageReq {
        shcode: String,
        #[serde(skip)]
        tr_cont: String,
        #[serde(skip)]
        tr_cont_key: String,
    }
    crate::impl_has_pagination!(PageReq);

    #[derive(Debug, Deserialize)]
    struct PageRes {
        #[serde(default)]
        rsp_cd: String,
        #[serde(default)]
        row: String,
        #[serde(default)]
        tr_cont: String,
        #[serde(default)]
        tr_cont_key: String,
    }
    crate::impl_has_pagination!(PageRes);

    /// Two-page responder: page 1 returns tr_cont="Y", page 2 returns "" / "N".
    struct PaginateResponder {
        hits: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        /// When true, never stops paginating (always returns tr_cont="Y").
        never_stop: bool,
    }

    impl Respond for PaginateResponder {
        fn respond(&self, _req: &Request) -> ResponseTemplate {
            let n = self.hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self.never_stop {
                return ResponseTemplate::new(200)
                    .insert_header("tr_cont", "Y")
                    .insert_header("tr_cont_key", "k")
                    .set_body_json(serde_json::json!({"rsp_cd": "00000", "row": format!("r{n}")}));
            }
            if n == 0 {
                ResponseTemplate::new(200)
                    .insert_header("tr_cont", "Y")
                    .insert_header("tr_cont_key", "page2key")
                    .set_body_json(serde_json::json!({"rsp_cd": "00000", "row": "r0"}))
            } else {
                ResponseTemplate::new(200)
                    .insert_header("tr_cont", "N")
                    .insert_header("tr_cont_key", "")
                    .set_body_json(serde_json::json!({"rsp_cd": "00000", "row": "r1"}))
            }
        }
    }

    fn page_policy() -> EndpointPolicy {
        EndpointPolicy {
            tr_code: "PAGE",
            path: "/page",
            has_pagination: true,
            ..test_policy()
        }
    }

    #[tokio::test]
    async fn collect_all_walks_two_pages() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        Mock::given(method("POST"))
            .and(path("/page"))
            .respond_with(PaginateResponder {
                hits: hits.clone(),
                never_stop: false,
            })
            .mount(&server)
            .await;

        let inner = Inner::new(mock_config(&server.uri())).expect("valid config");
        let inner2 = inner.clone();
        let req = PageReq {
            shcode: "005930".into(),
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        };
        let pages = inner
            .collect_all(req, move |r| {
                let inner = inner2.clone();
                async move {
                    inner
                        .post_paginated::<PageReq, PageRes>(&page_policy(), &r)
                        .await
                }
            })
            .await
            .expect("collect_all should walk two pages");
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].row, "r0");
        assert_eq!(pages[1].row, "r1");
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 2);
        // Page 1's tr_cont header was injected into the JSON so the getter works.
        assert_eq!(pages[0].tr_cont, "Y");
        assert_eq!(pages[0].tr_cont_key, "page2key");
        assert_eq!(pages[1].tr_cont, "N");
    }

    #[tokio::test]
    async fn collect_all_returns_pagination_limit_when_truncated() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        Mock::given(method("POST"))
            .and(path("/page"))
            .respond_with(PaginateResponder {
                hits: hits.clone(),
                never_stop: true,
            })
            .mount(&server)
            .await;

        // Cap at 3 pages; the server never stops → PaginationLimit(3).
        let inner = Inner::new(mock_config_max_pages(&server.uri(), 3)).expect("valid config");
        let inner2 = inner.clone();
        let req = PageReq {
            shcode: "005930".into(),
            tr_cont: String::new(),
            tr_cont_key: String::new(),
        };
        let err = inner
            .collect_all(req, move |r| {
                let inner = inner2.clone();
                async move {
                    inner
                        .post_paginated::<PageReq, PageRes>(&page_policy(), &r)
                        .await
                }
            })
            .await
            .expect_err("must hit the pagination cap");
        match err {
            LsError::PaginationLimit(n) => assert_eq!(n, 3),
            other => panic!("expected PaginationLimit(3), got {other:?}"),
        }
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[test]
    fn rsp_cd_success_classification() {
        assert!(rsp_cd_is_success(""));
        assert!(rsp_cd_is_success("00000"));
        assert!(rsp_cd_is_success("00136"));
        assert!(rsp_cd_is_success("00707"));
        assert!(!rsp_cd_is_success("01900"));
        assert!(!rsp_cd_is_success("IGW40013"));
    }

    // --- Order dispatch: the no-retry path + the order success predicate ------

    /// A minimal `is_order: true` policy for the order dispatch tests.
    fn order_policy() -> EndpointPolicy {
        EndpointPolicy {
            tr_code: "CSPAT00601",
            path: "/order/path",
            is_order: true,
            category: RateLimitCategory::Orders,
            ..test_policy()
        }
    }

    #[test]
    fn order_rsp_cd_classification() {
        // Seed accepts.
        assert_eq!(classify_order_rsp_cd("00039"), OrderAck::Accepted);
        assert_eq!(classify_order_rsp_cd("00040"), OrderAck::Accepted);
        // Generic-success codes are AMBIGUOUS for orders, never Rejected — the
        // double-fill guard, since the read path trusts 00000.
        assert_eq!(classify_order_rsp_cd("00000"), OrderAck::Ambiguous);
        assert_eq!(classify_order_rsp_cd(""), OrderAck::Ambiguous);
        // Read-success codes are NOT order-success — proves the read predicate
        // is not reused for orders.
        assert_eq!(classify_order_rsp_cd("00136"), OrderAck::Rejected);
        assert_eq!(classify_order_rsp_cd("00707"), OrderAck::Rejected);
        // A recognized rejection preserves code/message downstream.
        assert_eq!(classify_order_rsp_cd("IGW40011"), OrderAck::Rejected);
        // The seed predicate excludes the generic-success codes.
        assert!(rsp_cd_is_order_success("00039"));
        assert!(rsp_cd_is_order_success("00040"));
        assert!(!rsp_cd_is_order_success("00000"));
    }

    #[tokio::test]
    async fn post_order_buy_ack_00040_classifies_accepted_and_deserializes() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path("/order/path"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rsp_cd": "00040",
                "rsp_msg": "매수주문이 완료되었습니다",
                "value": "ordno-123"
            })))
            .mount(&server)
            .await;

        let inner = Inner::new(mock_config(&server.uri())).expect("valid config");
        // The defining assertion: an accepted ack deserializes into Res rather
        // than surfacing as ApiError — proving the order predicate runs INSIDE
        // dispatch_once, before Res deserialization.
        let res: EchoRes = inner
            .post_order(&order_policy(), &serde_json::json!({"OrdQty": 1}))
            .await
            .expect("00040 ack must classify Accepted and deserialize");
        assert_eq!(res.rsp_cd, "00040");
        assert_eq!(res.value, "ordno-123");
    }

    #[tokio::test]
    async fn post_order_sell_ack_00039_classifies_accepted() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path("/order/path"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rsp_cd": "00039",
                "rsp_msg": "매도주문이 완료되었습니다"
            })))
            .mount(&server)
            .await;

        let inner = Inner::new(mock_config(&server.uri())).expect("valid config");
        let res: EchoRes = inner
            .post_order(&order_policy(), &serde_json::json!({}))
            .await
            .expect("00039 ack must classify Accepted");
        assert_eq!(res.rsp_cd, "00039");
    }

    #[tokio::test]
    async fn post_order_read_success_codes_are_not_order_success() {
        // 00136/00707 are read-success codes; for an order they are rejections.
        for code in ["00136", "00707"] {
            let server = MockServer::start().await;
            mount_token(&server).await;
            Mock::given(method("POST"))
                .and(path("/order/path"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "rsp_cd": code,
                    "rsp_msg": "not an order ack"
                })))
                .mount(&server)
                .await;
            let inner = Inner::new(mock_config(&server.uri())).expect("valid config");
            let err = inner
                .post_order::<_, EchoRes>(&order_policy(), &serde_json::json!({}))
                .await
                .expect_err("read-success code must not be order-success");
            match err {
                LsError::ApiError { code: c, .. } => assert_eq!(c, code),
                other => panic!("expected ApiError for {code}, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn post_order_generic_success_00000_is_ambiguous_not_rejected() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path("/order/path"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rsp_cd": "00000",
                "rsp_msg": "정상처리"
            })))
            .mount(&server)
            .await;

        let inner = Inner::new(mock_config(&server.uri())).expect("valid config");
        let err = inner
            .post_order::<_, EchoRes>(&order_policy(), &serde_json::json!({}))
            .await
            .expect_err("00000 must not deserialize as a proven order accept");
        // KTD2 fail-safe: ambiguous, NOT a rejection — so a possibly-filled
        // order is reconciled, never resubmitted.
        match err {
            LsError::AmbiguousOrder { code, .. } => assert_eq!(code, "00000"),
            other => panic!("expected AmbiguousOrder, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn post_order_rejects_non_order_policy_before_any_http() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let hits_inner = hits.clone();
        Mock::given(method("POST"))
            .and(path("/test/path"))
            .respond_with(move |_req: &Request| {
                hits_inner.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"rsp_cd": "00000"}))
            })
            .mount(&server)
            .await;

        let inner = Inner::new(mock_config(&server.uri())).expect("valid config");
        // test_policy() is is_order:false — guard_order must reject it.
        let err = inner
            .post_order::<_, EchoRes>(&test_policy(), &serde_json::json!({}))
            .await
            .expect_err("non-order policy must be rejected by guard_order");
        match err {
            LsError::ApiError { code, .. } => assert_eq!(code, "non-order-dispatch"),
            other => panic!("expected guard_order ApiError, got {other:?}"),
        }
        assert_eq!(
            hits.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "guard_order must reject before any HTTP call"
        );
    }

    #[tokio::test]
    async fn post_order_does_not_retry_on_503_single_attempt() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        // 503 forever. `post` would retry up to 4 times; `post_order` must not.
        Mock::given(method("POST"))
            .and(path("/order/path"))
            .respond_with(FlakyResponder {
                hits: hits.clone(),
                fail_first: usize::MAX,
            })
            .mount(&server)
            .await;

        let inner = Inner::new(mock_config(&server.uri())).expect("valid config");
        let err = inner
            .post_order::<_, EchoRes>(&order_policy(), &serde_json::json!({}))
            .await
            .expect_err("503 must surface, not be retried");
        // The order body is empty JSON on a 503 → AmbiguousOrder (the order path
        // never collapses a non-2xx to a clean rejection).
        assert!(
            matches!(err, LsError::AmbiguousOrder { .. } | LsError::Http(_)),
            "expected ambiguous/transport error, got {err:?}"
        );
        // The defining difference from `post`: EXACTLY ONE attempt.
        let total = hits.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(total, 1, "order dispatch must issue exactly one attempt, got {total}");
    }

    #[tokio::test]
    async fn post_order_ack_on_non_2xx_status_is_ambiguous() {
        // An order ack code arriving on a non-2xx status routes ambiguous, not
        // Rejected — covers the second classification site.
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path("/order/path"))
            .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
                "rsp_cd": "00040",
                "rsp_msg": "ack on a failed status"
            })))
            .mount(&server)
            .await;

        let inner = Inner::new(mock_config(&server.uri())).expect("valid config");
        let err = inner
            .post_order::<_, EchoRes>(&order_policy(), &serde_json::json!({}))
            .await
            .expect_err("non-2xx must not deserialize as accepted");
        match err {
            LsError::AmbiguousOrder { code, .. } => assert_eq!(code, "00040"),
            other => panic!("expected AmbiguousOrder on non-2xx, got {other:?}"),
        }
    }

    // --- Kill switch (AE4) ----------------------------------------------------

    #[tokio::test]
    async fn kill_switch_halts_order_before_any_http_while_reads_succeed() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        let order_hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let order_hits_inner = order_hits.clone();
        Mock::given(method("POST"))
            .and(path("/order/path"))
            .respond_with(move |_req: &Request| {
                order_hits_inner.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"rsp_cd": "00040"}))
            })
            .mount(&server)
            .await;
        // A market-data read on the SAME Inner must keep working.
        Mock::given(method("POST"))
            .and(path("/test/path"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rsp_cd": "00000",
                "rsp_msg": "ok",
                "value": "still-reading"
            })))
            .mount(&server)
            .await;

        let inner = Inner::new(mock_config(&server.uri())).expect("valid config");
        inner.set_orders_enabled(false);
        assert!(!inner.orders_enabled());

        let err = inner
            .post_order::<_, EchoRes>(&order_policy(), &serde_json::json!({}))
            .await
            .expect_err("disabled orders must halt");
        match err {
            LsError::ApiError { code, .. } => assert_eq!(code, "orders-disabled"),
            other => panic!("expected orders-disabled ApiError, got {other:?}"),
        }
        assert_eq!(
            order_hits.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "kill switch must halt before any order HTTP call"
        );

        // The non-order read on the same Inner is unaffected.
        let res: EchoRes = inner
            .post(&test_policy(), &serde_json::json!({}))
            .await
            .expect("non-order read must be unaffected by the kill switch");
        assert_eq!(res.value, "still-reading");
    }

    #[tokio::test]
    async fn kill_switch_re_enable_restores_dispatch() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("POST"))
            .and(path("/order/path"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rsp_cd": "00040",
                "rsp_msg": "ack"
            })))
            .mount(&server)
            .await;

        let inner = Inner::new(mock_config(&server.uri())).expect("valid config");
        inner.set_orders_enabled(false);
        assert!(inner
            .post_order::<_, EchoRes>(&order_policy(), &serde_json::json!({}))
            .await
            .is_err());
        // Toggling mid-session is observed on the very next call.
        inner.set_orders_enabled(true);
        let res: EchoRes = inner
            .post_order(&order_policy(), &serde_json::json!({}))
            .await
            .expect("re-enabling must restore order dispatch");
        assert_eq!(res.rsp_cd, "00040");
    }

    // --- Dedup integration through post_order (AE1) ---------------------------

    #[tokio::test]
    async fn post_order_dedup_hit_returns_cached_and_skips_second_http() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let hits_inner = hits.clone();
        Mock::given(method("POST"))
            .and(path("/order/path"))
            .respond_with(move |_req: &Request| {
                hits_inner.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "rsp_cd": "00040",
                    "rsp_msg": "ack",
                    "value": "ordno-777"
                }))
            })
            .mount(&server)
            .await;

        let inner = Inner::new(mock_config(&server.uri())).expect("valid config");
        let req = serde_json::json!({"IsuNo": "005930", "OrdQty": 1, "OrdPrc": 100});

        let first: EchoRes = inner
            .post_order(&order_policy(), &req)
            .await
            .expect("first submit dispatches");
        assert_eq!(first.value, "ordno-777");

        // An identical request within the TTL is a cache hit: same response, no
        // second HTTP call.
        let second: EchoRes = inner
            .post_order(&order_policy(), &req)
            .await
            .expect("identical submit is a dedup hit");
        assert_eq!(second, first);
        assert_eq!(
            hits.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "the dedup hit must bypass HTTP entirely"
        );

        // A request differing only in OrdQty is a distinct order -> cache miss.
        let req2 = serde_json::json!({"IsuNo": "005930", "OrdQty": 2, "OrdPrc": 100});
        let _third: EchoRes = inner
            .post_order(&order_policy(), &req2)
            .await
            .expect("distinct order dispatches");
        assert_eq!(
            hits.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "a quantity change must miss the cache and dispatch"
        );
    }

    #[tokio::test]
    async fn post_order_does_not_cache_rejections() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let hits_inner = hits.clone();
        Mock::given(method("POST"))
            .and(path("/order/path"))
            .respond_with(move |_req: &Request| {
                hits_inner.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "rsp_cd": "IGW40011",
                    "rsp_msg": "rejected"
                }))
            })
            .mount(&server)
            .await;

        let inner = Inner::new(mock_config(&server.uri())).expect("valid config");
        let req = serde_json::json!({"OrdQty": 1});
        for _ in 0..2 {
            let err = inner
                .post_order::<_, EchoRes>(&order_policy(), &req)
                .await
                .expect_err("rejection");
            assert!(matches!(err, LsError::ApiError { .. }));
        }
        // A rejection is never cached: a corrected resubmission must reach the
        // exchange. Both attempts dispatched.
        assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    // --- Order redaction / tracing contract (order-safety §5) -----------------

    /// A process-global tracing layer that records every span/event field into a
    /// per-thread sink. A global subscriber is required (not a thread-local
    /// `with_default`) because tracing's callsite-interest cache is global: with
    /// the parallel test harness, a concurrent test would otherwise evaluate the
    /// order-span callsite under the no-op subscriber and poison it to "never",
    /// so a thread-local subscriber never gets consulted. Installing a permissive
    /// global once keeps every callsite enabled; each thread reads only its own
    /// sink, so concurrent tests do not interfere.
    mod capture {
        use std::cell::RefCell;
        use std::sync::OnceLock;
        use tracing::field::{Field, Visit};
        use tracing::span::{Attributes, Record};
        use tracing_subscriber::layer::{Context, Layer};
        use tracing_subscriber::layer::SubscriberExt;

        thread_local! {
            static SINK: RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
        }

        static INSTALLED: OnceLock<()> = OnceLock::new();

        /// Install the global capture subscriber exactly once.
        pub fn ensure_installed() {
            INSTALLED.get_or_init(|| {
                let subscriber = tracing_subscriber::registry().with(CaptureLayer);
                let _ = tracing::subscriber::set_global_default(subscriber);
            });
        }

        /// Clear the current thread's sink before a capture run.
        pub fn reset() {
            SINK.with(|s| s.borrow_mut().clear());
        }

        /// `name=value` of every field this thread captured.
        pub fn joined() -> String {
            SINK.with(|s| {
                s.borrow()
                    .iter()
                    .map(|(n, v)| format!("{n}={v}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
        }

        /// The field names this thread captured.
        pub fn field_names() -> Vec<String> {
            SINK.with(|s| s.borrow().iter().map(|(n, _)| n.clone()).collect())
        }

        struct V;
        impl Visit for V {
            fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
                SINK.with(|s| {
                    s.borrow_mut()
                        .push((field.name().to_string(), format!("{value:?}")))
                });
            }
            fn record_str(&mut self, field: &Field, value: &str) {
                SINK.with(|s| s.borrow_mut().push((field.name().to_string(), value.to_string())));
            }
        }

        struct CaptureLayer;
        impl<S> Layer<S> for CaptureLayer
        where
            S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
        {
            fn on_new_span(&self, attrs: &Attributes<'_>, _id: &tracing::Id, _c: Context<'_, S>) {
                attrs.record(&mut V);
            }
            fn on_record(&self, _id: &tracing::Id, values: &Record<'_>, _c: Context<'_, S>) {
                values.record(&mut V);
            }
            fn on_event(&self, event: &tracing::Event<'_>, _c: Context<'_, S>) {
                event.record(&mut V);
            }
        }
    }

    #[test]
    fn order_span_records_only_structural_fields_never_credentials_or_body() {
        capture::ensure_installed();
        // A current-thread runtime keeps every awaited span on this thread, so
        // this thread's sink captures exactly the order dispatch's fields.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        rt.block_on(async {
            let server = MockServer::start().await;
            mount_token(&server).await;
            // The response carries a sentinel that MUST NOT be auto-emitted to
            // any sink (only rsp_cd is an allowed structural field).
            Mock::given(method("POST"))
                .and(path("/order/path"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "rsp_cd": "00040",
                    "rsp_msg": "ack",
                    "OrdNo": "RESP_SENTINEL_ORDNO"
                })))
                .mount(&server)
                .await;

            let inner = Inner::new(mock_config(&server.uri())).expect("valid config");
            // Reset AFTER setup (token mount/Inner::new emit their own spans);
            // capture only the order dispatch itself.
            capture::reset();
            // The request body carries a sentinel that MUST NOT reach a span.
            let req = serde_json::json!({
                "IsuNo": "005930",
                "OrdQty": 1,
                "REQ_SENTINEL": "REQ_SENTINEL_BODY_VALUE"
            });
            let _res: EchoRes = inner
                .post_order(&order_policy(), &req)
                .await
                .expect("order dispatches");
        });

        let names = capture::field_names();
        // The four allowed structural fields were recorded on the order span.
        for f in ["tr_code", "path", "category", "dedup_hit"] {
            assert!(
                names.iter().any(|n| n == f),
                "order span must record `{f}`; got {names:?}"
            );
        }

        let all = capture::joined();
        // No credential VALUE reached any span or event.
        assert!(!all.contains("test-appkey"), "app key leaked: {all}");
        assert!(!all.contains("test-appsecretkey"), "app secret leaked");
        assert!(!all.contains("00000000-01"), "account number leaked");
        assert!(!all.contains("test-token"), "access token leaked");
        // No request body value reached any span or event.
        assert!(
            !all.contains("REQ_SENTINEL_BODY_VALUE"),
            "request body leaked into tracing: {all}"
        );
        // The response body is not auto-emitted absent an explicit surface call.
        assert!(
            !all.contains("RESP_SENTINEL_ORDNO"),
            "response body auto-emitted to a sink: {all}"
        );
    }
}
