//! Named consumer-side stream for typed WS subscriptions.
//!
//! `WsStream<Res>` is the nameable return of [`super::WsManager::subscribe_typed`]
//! — an `Either` of the two overflow-policy streams with an inline per-item
//! decode. Naming the type lets callers store the stream in struct fields.
//!
//! Yields `LsResult<Res>`. Each inbound frame body is decoded per item; a decode
//! failure yields `Err(LsError::Decode)` **without terminating the stream** — the
//! next frame is delivered normally. Registration-ACK frames carry a JSON `null`
//! body and surface as exactly such per-item errors.
//!
//! ## Terminal behavior
//!
//! Both policies share one terminal contract. On reconnect-budget exhaustion the
//! stream yields `Err(LsError::WebSocket("reconnect budget exhausted"))` and then
//! ends (yields `None`): under `DropNewest` the dispatch sender drops; under
//! `LatestOnly` the exhaustion path writes the error then closes the slot, so the
//! error is always drained before the end. Unsubscribing, dropping the handle, or
//! re-subscribing to the same key likewise ends the stream under both policies,
//! after any still-buffered value is drained.

use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::future::Either;
use futures::Stream;
use tokio_stream::wrappers::ReceiverStream;

use super::overflow::{LatestOnlySlot, LatestOnlyStream};
use ls_core::LsResult;

/// Typed market-data stream returned by [`super::WsManager::subscribe_typed`],
/// paired with a [`super::SubscriptionHandle`].
#[must_use = "streams do nothing unless polled"]
pub struct WsStream<Res> {
    inner: Either<ReceiverStream<LsResult<serde_json::Value>>, LatestOnlyStream>,
    /// `fn() -> Res` keeps `Send`/`Sync` unconditional in `Res` — the subscribe
    /// bound is only `Res: Send`, so a plain `PhantomData<Res>` would make
    /// `WsStream: Sync` depend on `Res: Sync`.
    _marker: PhantomData<fn() -> Res>,
}

impl<Res> WsStream<Res> {
    /// DropNewest arm — wraps the bounded dispatch channel receiver.
    pub(crate) fn drop_newest(
        rx: tokio::sync::mpsc::Receiver<LsResult<serde_json::Value>>,
    ) -> Self {
        Self {
            inner: Either::Left(ReceiverStream::new(rx)),
            _marker: PhantomData,
        }
    }

    /// LatestOnly arm. The stream must hold the only `Arc` of the slot: the
    /// dispatch map keeps a `Weak`, and consumer drop is detected when
    /// `Weak::upgrade` returns `None` — extra `Arc` clones held elsewhere would
    /// silently break ghost-subscription cleanup.
    pub(crate) fn latest_only(slot: Arc<LatestOnlySlot>) -> Self {
        Self {
            inner: Either::Right(LatestOnlyStream::new(slot)),
            _marker: PhantomData,
        }
    }
}

impl<Res> Stream for WsStream<Res>
where
    Res: serde::de::DeserializeOwned,
{
    type Item = LsResult<Res>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // All fields are Unpin (ReceiverStream and LatestOnlyStream hold no
        // self-references), so no structural pin projection is needed.
        let this = self.get_mut();
        Pin::new(&mut this.inner).poll_next(cx).map(|item| {
            item.map(|result| {
                result.and_then(|v| {
                    serde_json::from_value::<Res>(v).map_err(ls_core::LsError::Decode)
                })
            })
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

impl<Res> std::fmt::Debug for WsStream<Res> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let policy = match &self.inner {
            Either::Left(_) => "DropNewest",
            Either::Right(_) => "LatestOnly",
        };
        f.debug_struct("WsStream")
            .field("policy", &policy)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Send + DeserializeOwned + 'static` but NOT `Sync` — proves the
    /// `PhantomData<fn() -> Res>` marker keeps `WsStream: Sync` even when `Res`
    /// itself is not.
    #[derive(serde::Deserialize)]
    struct NotSyncRes {
        #[serde(skip)]
        _not_sync: std::cell::Cell<u8>,
    }

    /// Compile-time contract: the named type must preserve `Stream<Item =
    /// LsResult<Res>>` plus `Send + Sync + Unpin + 'static`.
    fn assert_ws_stream_contract<Res>()
    where
        Res: serde::de::DeserializeOwned + Send + 'static,
        WsStream<Res>: Stream<Item = LsResult<Res>> + Send + Sync + Unpin + 'static,
    {
    }

    #[test]
    fn ws_stream_preserves_auto_traits() {
        assert_ws_stream_contract::<NotSyncRes>();
        assert_ws_stream_contract::<serde_json::Value>();
    }
}
