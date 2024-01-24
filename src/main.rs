use std::fmt::Debug;
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
use tracing::field::Field;

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
    span: String,
}

#[derive(Deserialize, Serialize, Debug)]
struct CustomVisitor {
    message: String,
}


impl CustomVisitor {
    fn new() -> Self {
        CustomVisitor{ message: "".to_string()  }
    }
}

impl tracing::field::Visit for CustomVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        if field.to_string() == "message" {
            self.message =format!("{value:?}");
        }
    }
}

impl std::fmt::Display for CustomVisitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(&self).unwrap_or_default())
    }
}



impl TracingMessage {
    fn new(event: &tracing::Event<'_>, span_name: &str) -> Self {
        let metadata = event.metadata();
        let _fields = event.fields();


        let mut visitor = CustomVisitor::new();
        event.record(&mut visitor);

        TracingMessage {
            name: metadata.name().to_string(),
            level: metadata.level().to_string(),
            module_path: metadata.module_path().unwrap().to_string(),
            message: visitor.message,
            timestamp: Utc::now().timestamp_millis(),
            span: span_name.to_string(),
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

        let span = ctx.event_span(event).unwrap();

        let message = TracingMessage::new(event, span.name());
        let tx = self.tx.clone();
        tokio::spawn(async move {
            if (tx.send(message).await).is_err() {
                println!("receiver dropped");
            };

            tx.closed().await
        });

    }
}

#[instrument(name = "pheidippides")]
async fn run_forever() {
    loop {
        sleep(Duration::from_millis(20));
        println!("Sending logmessage {}", Utc::now().timestamp_millis());
        info!("Joy, we win!");
    }
}

#[tokio::main]
async fn main() {
    tokio::spawn(async move {
        tcp_serv_listen(TcpListener::bind("127.0.0.1:5555").await.unwrap()).await
    });

    let (tx, mut rx) = mpsc::channel(100000);

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
