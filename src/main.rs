use tokio::signal;
use tracing::info;

/// REST call response for GET IP address
#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct GetIpResponse {
    origin: String,
}

pub type URIIn = tokio::sync::mpsc::Sender<(u64, String)>;
pub type WebResultRx = tokio::sync::mpsc::Sender<(u64, String)>;

/// RESTMultiplexer utilizes the simple Reqwest interaction, but via a battery of
/// tokio tasks, in order to execute in parallel.
///
/// By using tokio tasks we achieve both potential same-thread multiplexing as well
/// as potential thread-concurrency depending on the load & scheduler.
///
/// The max number of channels to create is configurable in the multiplexer constructor.
/// This number will affect the number of outstanding requests we attempt to hold
/// open with the exchange at any given point in time.
struct RESTMultiplexer {
    channels: Vec<URIIn>,
    next_channel: usize,
}

impl RESTMultiplexer {
    pub async fn new(response_out: WebResultRx, max_concurrent_outbound_txn: u64) -> Self {
        let mut channels = Vec::new();

        // Instantiate all the separate channels
        for _ in 0..max_concurrent_outbound_txn {
            channels.push(Self::init_rest_channel(response_out.clone()));
        }

        Self {
            channels,
            next_channel: 0,
        }
    }

    // Drop a fetch request for the given URI into out channel battery
    pub async fn fetch(&mut self, req_num: u64, uri: &str) {
        let sending_channel = self.next_channel;

        // Post the URI to fetch to the next channel (round robin)...
        self.channels[sending_channel]
            .send((req_num, uri.to_string()))
            .await
            .unwrap();

        // Round-robin for the next fetch
        self.next_channel += 1;
        if self.next_channel >= self.channels.len() {
            self.next_channel = 0;
        }
    }

    fn init_rest_channel(response_channel: WebResultRx) -> URIIn {
        let (request_in, mut request_in_rx) = tokio::sync::mpsc::channel::<(u64, String)>(10);

        tokio::spawn(async move {
            loop {
                // Only one branch here at the moment, but you could imaging using select here for catching other signals,
                // such as a shutdown signal.
                tokio::select! {
                    // Pick up the URL and initiate the GET request
                    Some(req) = request_in_rx.recv() => {
                        let resp = match reqwest::get(req.1.to_string()).await {
                            Ok(resp) => resp.text().await.unwrap(),
                            Err(err) => panic!("Error: {}", err)
                        };

                        // extract the "origin" field (IP) from the json response
                        let ip_response = match serde_json::from_str::<GetIpResponse>(&resp) {
                            Ok(origin) => origin,
                            Err(err) => panic!("Error: {}", err)
                        };

                        // Send on the response string (+ req #) to the result channel.
                        response_channel.send((req.0, ip_response.origin)).await.unwrap();
                    }
                }
            }
        });

        request_in
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn run(max_concurrent_outbound_txn: u64) {
    // Create a channel for the response from the REST calls
    let (response_out, mut response_out_rx) = tokio::sync::mpsc::channel::<(u64, String)>(10);

    // Create the multiplexer for the REST calls
    let mut rest_multiplexer =
        RESTMultiplexer::new(response_out, max_concurrent_outbound_txn).await;

    // Create a channel for the source input
    let (source_in, mut source_in_rx) = tokio::sync::mpsc::channel::<u64>(10);

    // task to receive
    tokio::spawn(async move {
        loop {
            tokio::select! {
                // match on msg received
                Some(msg) = source_in_rx.recv() => {
                    info!("Rcvd msg {} on thread {}", msg, thread_id::get());
                    rest_multiplexer.fetch(msg, "https://httpbin.org/ip").await;
                },

                Some(rest_resp) = response_out_rx.recv() => {
                    info!("REST rsp {} on thread {} = {}", rest_resp.0, thread_id::get(), rest_resp.1 );
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
    run(5);
}
