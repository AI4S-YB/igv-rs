use std::convert::Infallible;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Router,
};
use futures::stream::Stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::state::ServerState;

pub fn router() -> Router<ServerState> {
    Router::new().route("/api/sse", get(handler))
}

async fn handler(
    State(s): State<ServerState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = s.events.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|res| match res {
        Ok(ev) => Some(Ok(Event::default()
            .event("view")
            .json_data(ev)
            .expect("ViewEvent serializes to JSON"))),
        Err(_lag) => None, // drop lagged events silently
    });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)))
}
