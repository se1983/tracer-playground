use std::thread::sleep;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, instrument};

use tracing_subscriber::fmt::{self, format, time};
use tracing_subscriber::prelude::*;

use chrono::prelude::*;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Sender;

use colored_json::prelude::*;

use serde::{Deserialize, Serialize};
use tracing_subscriber::registry::SpanRef;

async fn tcp_serv_listen(listener: TcpListener) {
    println!("Listening");

    loop {
        let (mut socket, _) = listener.accept().await.unwrap();

        tokio::spawn(async move {
            let mut buf = vec![0; 1024];

            loop {
                let n = socket
                    .read(&mut buf)
                    .await
                    .expect("failed to read data from socket");

                if n == 0 {
                    return;
                }

                let i = socket.read(&mut buf).await.unwrap();
                let data = String::from_utf8_lossy(&buf[..i]);

                let output = data.to_colored_json_auto().unwrap();
                println!("{output}");
            }
        });
    }
}

#[derive(Deserialize, Serialize, Debug)]
struct TracingMessage {
    name: String,
    level: String,
    module_path: String,
    message: String,
    timestamp: i64,
}

impl TracingMessage {
    fn new(event: &tracing::Event<'_>, span_name: String) -> Self {
        let metadata = event.metadata();
        let fields = event.fields();

        TracingMessage {
            name: metadata.name().to_string(),
            level: metadata.level().to_string(),
            module_path: metadata.module_path().unwrap().to_string(),
            message: fields.map(|m| m.to_string()).collect(),
            timestamp: Utc::now().timestamp_millis(),
        }
    }
}

impl std::fmt::Display for TracingMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(&self).unwrap_or_default())
    }
}

struct TCPTracingLayer {
    tx: Sender<TracingMessage>,
}

impl TCPTracingLayer {
    async fn new(_addr: &str, tx: Sender<TracingMessage>) -> Self {
        TCPTracingLayer { tx }
    }
}

impl<S> tracing_subscriber::Layer<S> for TCPTracingLayer
where
    S: tracing::Subscriber,
    S: for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {

        futures::executor::block_on(async move {
            if let Err(_) = self.tx.clone().send(TracingMessage::new(event, "hi".to_string())).await {
                println!("receiver dropped");
            };
        });
    }
}

#[instrument(name = "pheidippides")]
async fn run_forever() {
    loop {
        sleep(Duration::from_millis(2000));
        println!("Sending logmessage {}", Utc::now().timestamp_millis());
        info!("Joy, we win!");
    }
}

#[tokio::main]
async fn main() {
    tokio::spawn(async move {
        tcp_serv_listen(TcpListener::bind("127.0.0.1:5555").await.unwrap()).await
    });

    let (tx, mut rx) = mpsc::channel(10);

    let fmt = format().with_timer(time::Uptime::default());
    let fmt_layer = fmt::layer().event_format(fmt).with_target(false);

    tracing_subscriber::registry()
        .with(TCPTracingLayer::new("127.0.0.1:5555", tx).await)
        .with(fmt_layer)
        .init();

    tokio::spawn(async move { run_forever().await });

    let mut sender = TcpStream::connect("127.0.0.1:5555").await.unwrap();
    while let Some(msg) = rx.recv().await {
        sender.write_all(msg.to_string().as_bytes()).await.unwrap();
    }
}
