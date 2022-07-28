use futures::stream::FuturesOrdered;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use reqwest::Client;
use reqwest_utils::*;
use tokio::{signal, time::sleep};
use tracing::{error, info};

mod reqwest_utils;

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

    // List of pending responses
    let mut wait_list_ordered = FuturesOrdered::new();
    let mut wait_list_unordered = FuturesUnordered::new();

    // Use web service with deliberate artificial delay in response.
    let uri = "https://httpbin.org/delay/5";

    // task to receive
    tokio::spawn(async move {
        // HTTP client
        let client = Client::new();

        loop {
            tokio::select! {
                // match on msg received
                Some(msg_id) = source_in_rx.recv() => {
                    info!("Rcvd msg {} on thread {}", msg_id, thread_id::get());

                    // Prepare the request
                    let req = ExchHttpFuture::new(
                                msg_id,
                                Box::pin(client.get(uri).send()),
                    );
                    wait_list_ordered.push( req );

                    // Prepare the request
                    let req_unord = ExchHttpFuture::new(
                        msg_id,
                        Box::pin(client.get(uri).send()),
                    );
                    wait_list_unordered.push( req_unord );
                    info!("Sent request to {}", uri);
                },

                // Watch the set of futures pushed to the wait_list
                Some(rest_resp_ord) = wait_list_ordered.next() => {
                    // extract the "origin" field (IP) from the json response
                    let ip_response = rest_resp_ord.response_result.unwrap().json::<GetIpResponse>().await;

                    // send the response to the source_out channel
                    if let Ok(origin) = ip_response.map(|r| r.origin) {
                        info!("ORD   REST msg {} received, on thread {} = {}", rest_resp_ord.request, thread_id::get(), origin );
                    } else {
                        error!("Failed to extract IP from response");
                    }
                },

                // Watch the set of futures pushed to the wait_list
                Some(rest_resp_unord) = wait_list_unordered.next() => {
                    // extract the "origin" field (IP) from the json response
                    let ip_response = rest_resp_unord.response_result.unwrap().json::<GetIpResponse>().await;

                    // send the response to the source_out channel
                    if let Ok(origin) = ip_response.map(|r| r.origin) {
                        info!("UNORD REST msg {} received, on thread {} = {}", rest_resp_unord.request, thread_id::get(), origin );
                    } else {
                        error!("Failed to extract IP from response");
                    }
                }
            }
        }
    });

    // task to send
    tokio::spawn(async move {
        // send 10, all within 1ms
        for i in 0..30 {
            info!("Send msg {} on thread {}", i, thread_id::get());
            source_in.send(i).await.unwrap();
        }

        sleep(std::time::Duration::from_secs(1)).await;

        // send 10, all within 1ms
        for i in 100..130 {
            info!("BATCH 2: Send msg {} on thread {}", i, thread_id::get());
            source_in.send(i).await.unwrap();
        }

        signal::ctrl_c()
            .await
            .expect("Failed to wait for ctrl-C signal.");
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
