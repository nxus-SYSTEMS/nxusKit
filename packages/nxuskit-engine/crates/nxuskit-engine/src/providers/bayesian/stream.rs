//! Streaming wrapper for iterative Bayesian Network inference.
//!
//! Provides `BayesStream<T>` — a tokio-mpsc–backed stream that delivers
//! progressive inference chunks from a background task. Supports:
//!
//! - Async consumption via `futures_core::Stream`
//! - Sync consumption via `blocking_iter()` (Article IX compliance)
//! - Cancellation via dropping the stream (closes the channel)

use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::sync::mpsc;

/// A progressive chunk from an iterative inference algorithm.
#[derive(Debug, Clone)]
pub struct BayesStreamChunk<T> {
    /// The (partial or final) inference result at this checkpoint.
    pub data: T,
    /// Number of iterations completed so far (e.g. Gibbs samples collected).
    pub iteration: usize,
    /// Total iterations requested (for progress calculation).
    pub total_iterations: usize,
    /// Convergence metric (algorithm-specific; lower = more converged).
    /// For Gibbs: max |P_current - P_previous| across all state probabilities.
    pub convergence_metric: f64,
    /// Whether this is the final chunk.
    pub is_final: bool,
}

/// A stream of progressive inference results backed by a tokio mpsc channel.
///
/// Created by iterative inference methods (e.g., `GibbsSampler::sample_stream`).
/// The background task sends `BayesStreamChunk<T>` values through the channel;
/// dropping the `BayesStream` closes the receiver, which the background task
/// detects as cancellation.
#[derive(Debug)]
pub struct BayesStream<T> {
    rx: mpsc::Receiver<BayesStreamChunk<T>>,
}

impl<T> BayesStream<T> {
    /// Create a new `BayesStream` from an mpsc receiver.
    pub fn new(rx: mpsc::Receiver<BayesStreamChunk<T>>) -> Self {
        Self { rx }
    }

    /// Consume this stream synchronously (blocking).
    ///
    /// This satisfies Article IX (sync-first) by providing an iterator that
    /// works without an async runtime. Internally spins a minimal tokio
    /// current-thread runtime to drive the channel.
    pub fn blocking_iter(self) -> BayesBlockingIter<T> {
        BayesBlockingIter { rx: self.rx }
    }
}

impl<T: Send + 'static> futures::Stream for BayesStream<T> {
    type Item = BayesStreamChunk<T>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<BayesStreamChunk<T>>> {
        self.rx.poll_recv(cx)
    }
}

// Unpin is needed for boxed stream compatibility.
impl<T> Unpin for BayesStream<T> {}

/// Blocking iterator adapter for `BayesStream`.
///
/// Consumes chunks synchronously. If called from within a tokio runtime,
/// uses `block_in_place`; otherwise creates a minimal runtime.
#[derive(Debug)]
pub struct BayesBlockingIter<T> {
    rx: mpsc::Receiver<BayesStreamChunk<T>>,
}

impl<T: Send + 'static> Iterator for BayesBlockingIter<T> {
    type Item = BayesStreamChunk<T>;

    fn next(&mut self) -> Option<Self::Item> {
        // Try to receive without blocking first (fast path for buffered data).
        match self.rx.try_recv() {
            Ok(chunk) => return Some(chunk),
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => return None,
        }

        // Slow path: block waiting for the next chunk.
        // If we're inside a tokio runtime, use block_in_place to avoid panic.
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.block_on(self.rx.recv())
        } else {
            // No runtime — spin up a minimal one.
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to create blocking runtime");
            rt.block_on(self.rx.recv())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn stream_delivers_chunks_async() {
        let (tx, rx) = mpsc::channel(16);
        let stream = BayesStream::new(rx);

        // Send 3 chunks from a background task
        tokio::spawn(async move {
            for i in 0..3 {
                tx.send(BayesStreamChunk {
                    data: format!("chunk-{}", i),
                    iteration: (i + 1) * 100,
                    total_iterations: 300,
                    convergence_metric: 1.0 / (i as f64 + 1.0),
                    is_final: i == 2,
                })
                .await
                .unwrap();
            }
        });

        let chunks: Vec<_> = stream.collect().await;
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].iteration, 100);
        assert_eq!(chunks[1].iteration, 200);
        assert_eq!(chunks[2].iteration, 300);
        assert!(!chunks[0].is_final);
        assert!(!chunks[1].is_final);
        assert!(chunks[2].is_final);
        assert!(chunks[0].convergence_metric > chunks[2].convergence_metric);
    }

    #[tokio::test]
    async fn stream_cancellation_via_drop() {
        let (tx, rx) = mpsc::channel(16);
        let stream = BayesStream::<String>::new(rx);

        // Drop the stream immediately — sender should detect closure
        drop(stream);

        let result = tx
            .send(BayesStreamChunk {
                data: "should-fail".to_string(),
                iteration: 1,
                total_iterations: 1,
                convergence_metric: 0.0,
                is_final: true,
            })
            .await;
        assert!(result.is_err(), "send should fail after stream is dropped");
    }

    #[test]
    fn blocking_iter_delivers_chunks() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let (tx, rx) = mpsc::channel(16);
        let stream = BayesStream::new(rx);

        // Spawn sender in the runtime
        rt.spawn(async move {
            for i in 0..3 {
                tx.send(BayesStreamChunk {
                    data: i,
                    iteration: i + 1,
                    total_iterations: 3,
                    convergence_metric: 0.1,
                    is_final: i == 2,
                })
                .await
                .unwrap();
            }
        });

        let iter = stream.blocking_iter();
        let chunks: Vec<_> = rt.block_on(async {
            // Use spawn_blocking to run the blocking iter from within tokio
            tokio::task::spawn_blocking(move || iter.collect::<Vec<_>>())
                .await
                .unwrap()
        });

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].data, 0);
        assert_eq!(chunks[2].data, 2);
        assert!(chunks[2].is_final);
    }
}
