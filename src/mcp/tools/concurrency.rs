//! Bounded-concurrency fan-out. Direct port of server.mjs's mapLimit, plain
//! HTTP fan-out, not CPU work, so concurrency is what turns a 36-course
//! sequential scan from minutes into seconds and avoids a client's 60s
//! tool-call timeout.

use futures::stream::{self, StreamExt};
use std::future::Future;

pub async fn map_limit<T, F, Fut>(items: Vec<T>, limit: usize, f: F)
where
    T: Send + 'static,
    F: Fn(T) -> Fut + Send + Sync,
    Fut: Future<Output = ()> + Send,
{
    stream::iter(items)
        .for_each_concurrent(Some(limit.max(1)), f)
        .await;
}
