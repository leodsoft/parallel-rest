use futures::stream::FuturesUnordered;
use futures::StreamExt;
use reqwest::Client;
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

    // List of pending responses
    let mut wait_list = FuturesUnordered::new();

    // Use web service with deliberate artificial delay in response.
    let uri = "https://httpbin.org/delay/5";

    // task to receive
    tokio::spawn(async move {
        // HTTP client
        let client = Client::new();

        loop {
            tokio::select! {
                // match on msg received
                Some(msg) = source_in_rx.recv() => {
                    info!("Rcvd msg {} on thread {}", msg, thread_id::get());

                    wait_list.push( client.get(uri).send() );
                    info!("Sent request to {}", uri);
                },

                // Watch the set of futures pushed to the wait_list
                Some(rest_resp) = wait_list.next() => {
                    // extract the "origin" field (IP) from the json response
                    let ip_response = rest_resp.unwrap().json::<GetIpResponse>().await;

                    // send the response to the source_out channel
                    if let Ok(origin) = ip_response.map(|r| r.origin) {
                        info!("REST response received, on thread {} = {}", thread_id::get(), origin );
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
        for i in 0..10 {
            info!("Send msg {} on thread {}", i, thread_id::get());
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
