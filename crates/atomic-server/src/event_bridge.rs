//! Bridge between atomic-core callback events and the server broadcast channel
//!
//! Provides helper functions that create callback closures which forward
//! EmbeddingEvent and ChatEvent instances into the tokio broadcast channel
//! as ServerEvent variants.

use crate::state::ServerEvent;
use tokio::sync::broadcast;

/// Create an EmbeddingEvent callback that broadcasts to WebSocket clients
pub fn embedding_event_callback(
    tx: broadcast::Sender<ServerEvent>,
) -> impl Fn(atomic_core::EmbeddingEvent) + Send + Sync + Clone + 'static {
    move |event: atomic_core::EmbeddingEvent| {
        let _ = tx.send(ServerEvent::from(event));
    }
}

/// Create a ChatEvent callback that broadcasts to WebSocket clients
pub fn chat_event_callback(
    tx: broadcast::Sender<ServerEvent>,
) -> impl Fn(atomic_core::ChatEvent) + Send + Sync + 'static {
    move |event: atomic_core::ChatEvent| {
        let _ = tx.send(ServerEvent::from(event));
    }
}
