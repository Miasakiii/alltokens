use serde::Serialize;
use std::sync::OnceLock;
use tokio::sync::broadcast;

static EVENT_BUS: OnceLock<EventBus> = OnceLock::new();

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WsEvent {
    ScanComplete { total: usize },
}

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<WsEvent>,
}

impl EventBus {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(32);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WsEvent> {
        self.tx.subscribe()
    }

    pub fn emit(&self, event: WsEvent) {
        let _ = self.tx.send(event);
    }

    pub fn emit_scan_complete(&self, total: usize) {
        self.emit(WsEvent::ScanComplete { total });
    }
}

pub fn install_event_bus(bus: EventBus) {
    let _ = EVENT_BUS.set(bus);
}

pub fn event_bus() -> Option<EventBus> {
    EVENT_BUS.get().cloned()
}

pub async fn notify_running_servers(total: usize) {
    let body = format!(r#"{{"total":{total}}}"#);
    let request = format!(
        "POST /api/events/scan-complete HTTP/1.1\r\n\
         Host: 127.0.0.1\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n\
         {}",
        body.len(),
        body
    );

    for port in [3210u16, 3212] {
        if let Ok(mut stream) =
            tokio::net::TcpStream::connect(format!("127.0.0.1:{port}")).await
        {
            use tokio::io::AsyncWriteExt;
            if stream.write_all(request.as_bytes()).await.is_ok() {
                break;
            }
        }
    }
}
