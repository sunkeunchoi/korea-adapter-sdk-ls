//! Pagination continuation contract for paginated REST TRs.
//!
//! `HasPagination` is implemented by paginated request wrapper structs (which
//! live in `ls-sdk`) for TRs that support `tr_cont`/`tr_cont_key` continuation.
//! Non-paginated TRs (token, revoke, and any standalone TR) are structurally
//! incapable of pagination and do NOT implement this trait.
//!
//! [`collect_all`](crate::Inner::collect_all) drives pagination through this trait:
//!   - reads `tr_cont`/`tr_cont_key` from response headers via `dispatch_once`
//!   - sets them on a cloned request struct before the next page call
//!   - repeats until the `tr_cont` response header is empty or `max_pages` is hit.

/// Pagination continuation for a paginated REST request struct.
///
/// Implemented on the outer `{TrCode}Request` wrapper struct (NOT the InBlock)
/// so the continuation fields never serialize into the request body.
/// `tr_cont`/`tr_cont_key` are carried as HTTP request headers by
/// `dispatch_once` when they are non-empty.
pub trait HasPagination {
    /// Returns the current `tr_cont` continuation value (empty string = first page).
    fn tr_cont(&self) -> &str;
    /// Returns the current `tr_cont_key` continuation value.
    fn tr_cont_key(&self) -> &str;
    /// Sets the `tr_cont` field for the next page request.
    fn set_tr_cont(&mut self, v: String);
    /// Sets the `tr_cont_key` field for the next page request.
    fn set_tr_cont_key(&mut self, v: String);
}

/// Implement [`HasPagination`] for a paginated request wrapper struct.
///
/// The struct must carry `tr_cont: String` and `tr_cont_key: String` fields.
/// Paginated request structs live in `ls-sdk` and invoke this as
/// `ls_core::impl_has_pagination!(<Req>);`, so the macro is `#[macro_export]`
/// (exported at the crate root) rather than crate-internal.
#[macro_export]
macro_rules! impl_has_pagination {
    ($t:ty) => {
        impl $crate::HasPagination for $t {
            fn tr_cont(&self) -> &str {
                &self.tr_cont
            }
            fn tr_cont_key(&self) -> &str {
                &self.tr_cont_key
            }
            fn set_tr_cont(&mut self, v: String) {
                self.tr_cont = v;
            }
            fn set_tr_cont_key(&mut self, v: String) {
                self.tr_cont_key = v;
            }
        }
    };
}
