use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use tokio::sync::broadcast;

use crate::event::{Body, Envelope};

#[derive(Clone)]
pub struct App {
    pub events: broadcast::Sender<Envelope>,
    pub snapshot: Arc<Mutex<HashMap<String, String>>>
}

pub async fn serve(bind: String, app: App) -> std::io::Result<()> {
    let router = Router::new()
        .route("/", get(index))
        .route("/ws", get(ws_upgrade))
        .with_state(app);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    eprintln!("hub ui  http://{bind}");
    eprintln!("hub ws  ws://{bind}/ws");
    axum::serve(listener, router).await
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(app): State<App>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| client(socket, app))
}

async fn client(mut socket: WebSocket, app: App) {
    {
        let devices = app.snapshot.lock().expect("snapshot").clone();
        let msg = serde_json::json!({ "v": 1, "kind": "snapshot", "devices": devices });
        if socket
            .send(Message::Text(msg.to_string().into()))
            .await
            .is_err()
        {
            return;
        }
    }

    let mut rx = app.events.subscribe();
    loop {
        tokio::select! {
            result = rx.recv() => {
                let Ok(env) = result else { continue };
                let Ok(text) = serde_json::to_string(&env) else { continue };
                if socket.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

pub fn apply_snapshot(snapshot: &Mutex<HashMap<String, String>>, env: &Envelope) {
    let mut map = snapshot.lock().expect("snapshot");
    match &env.body {
        Body::DeviceConnected { port } => {
            map.insert(env.device.clone(), port.clone());
        }
        Body::DeviceDisconnected { .. } => {
            map.remove(&env.device);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::apply_snapshot;
    use crate::event::{Envelope, Event};
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[test]
    fn connect_then_disconnect_updates_map() {
        let snapshot = Mutex::new(HashMap::new());
        let connected = Envelope::from_event(Event::DeviceConnected {
            device: "scanner-1".into(),
            port: "COM10".into(),
        });
        apply_snapshot(&snapshot, &connected);
        assert_eq!(snapshot.lock().unwrap().get("scanner-1").unwrap(), "COM10");

        let gone = Envelope::from_event(Event::DeviceDisconnected {
            device: "scanner-1".into(),
            reason: "unplug".into(),
        });
        apply_snapshot(&snapshot, &gone);
        assert!(snapshot.lock().unwrap().is_empty());
    }

    #[test]
    fn scan_does_not_touch_devices() {
        let snapshot = Mutex::new(HashMap::new());
        let scan = Envelope::from_event(Event::Scan {
            device: "scanner-1".into(),
            code: "1".into(),
        });
        apply_snapshot(&snapshot, &scan);
        assert!(snapshot.lock().unwrap().is_empty());
    }
}