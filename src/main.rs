use std::error::Error;
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, instrument};
use tracing_subscriber;

use tracing_subscriber::fmt::{self, format, SubscriberBuilder, time};
use tracing_subscriber::prelude::*;

use chrono::prelude::*;
use tracing::instrument::WithSubscriber;
use tracing_subscriber::layer::Context;

use tracing_subscriber::prelude::*;

async fn tcp_serv_listen(listener: TcpListener) {
    println!("Listening");

    while let (mut socket, _) = listener.accept().await.unwrap() {
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
                println!("{data:?}");
            }
        });
    }
}

struct TCPTracingLayer {
    connection: Arc<Mutex<TcpStream>>,
}

impl TCPTracingLayer {
    async fn new(addr: &str) -> Self {
        TCPTracingLayer {
            connection: Arc::new(Mutex::new(TcpStream::connect(addr).await.unwrap())),
        }
    }
}


impl<S> tracing_subscriber::Layer<S> for TCPTracingLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let timestamp = Utc::now().timestamp_millis();

        futures::executor::block_on(async move {
            let now = Utc::now().timestamp_millis();

            self.connection
                .lock()
                .unwrap()
                .write_all(format!("{event:?}",).as_bytes())
                .await.unwrap();
        });
    }

}

#[instrument(name = "my_span")]
async fn run_forever() {
    loop {
        sleep(Duration::from_millis(2000));
        println!("Sending logmessage {}", Utc::now().timestamp_millis());
        info!("We've got 3 teams!");
    }
}

#[tokio::main]
async fn main() {
    let listener = TcpListener::bind("127.0.0.1:5555").await.unwrap();
    tokio::spawn(async move { tcp_serv_listen(listener).await });

    let fmt = format().with_timer(time::Uptime::default());
    let fmt_layer = fmt::layer().event_format(fmt).with_target(false);

    tracing_subscriber::registry()
        .with(TCPTracingLayer::new("127.0.0.1:5555").await)
        .with(fmt_layer)
        .init();

    run_forever().await;
}
