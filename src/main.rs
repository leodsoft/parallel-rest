use futures::stream::FuturesUnordered;
use futures::StreamExt;
use isahc::prelude::*;
use tokio::signal;
use tracing::{error, info};

/// REST call response for GET IP address
#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct GetIpResponse {
    origin: String,
}

pub type URIIn = tokio::sync::mpsc::Sender<(u64, String)>;
pub type WebResultRx = tokio::sync::mpsc::Sender<(u64, String)>;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn run() {
    // Create a channel for the source input
    let (source_in, mut source_in_rx) = tokio::sync::mpsc::channel::<u64>(10);

    let mut wait_list = FuturesUnordered::new();

    // task to receive
    tokio::spawn(async move {
        loop {
            tokio::select! {
                // match on msg received
                Some(msg) = source_in_rx.recv() => {
                    info!("Rcvd msg {} on thread {}", msg, thread_id::get());
                    let response = isahc::get_async("https://httpbin.org/ip").await;
                    wait_list.push( async move {response} );
                },
                Some(rest_resp) = wait_list.next() => {
                     match rest_resp {
                         Ok(mut resp) => {
                            // extract the "origin" field (IP) from the json response
                            let ip_response = match serde_json::from_str::<GetIpResponse>(&resp.text().await.unwrap()) {
                                Ok(origin) => origin,
                                Err(err) => panic!("Error: {}", err)
                            };

                            // We are not passing the calling index
                            info!("REST RSP ? on thread {} = {}", thread_id::get(), ip_response.origin );
                         }
                         Err(err) => {
                             error!("{}", err);
                         }
                     }
                }
            }
        }
    });

    // task to send
    tokio::spawn(async move {
        // send 10, all within 1ms
        for i in 0..10 {
            info!("Send msg {} on thread {}", i, thread_id::get());
            source_in.send(i).await.unwrap();
        }
    });

    // Wait for the shutdown signal.
    println!("\nHit ctrl-C to end when ready...\n\n");
    signal::ctrl_c()
        .await
        .expect("Failed to wait for ctrl-C signal.");
}

/// main function
fn main() {
    tracing_subscriber::fmt::init();
    run();
}
