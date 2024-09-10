use std::{
    net::{TcpStream, ToSocketAddrs},
    num::NonZeroUsize,
    sync::mpsc,
    time::Duration,
};

use jsonlrpc::{JsonlStream, MaybeBatch, RequestObject, ResponseObject};
use orfail::{Failure, OrFail};
use serde::{Deserialize, Serialize};

/// Execute a stream of JSON-RPC calls received from the standard input.
#[derive(Debug, clap::Args)]
pub struct StreamCallCommand {
    /// JSON-RPC server address or hostname.
    server_addr: String,

    /// Additional JSON-RPC servers to execute the calls in parallel.
    additional_server_addrs: Vec<String>,

    /// Maximum number of concurrent calls for each server.
    #[clap(long, default_value = "1")]
    pipelining: NonZeroUsize,

    /// Output metrics about the calls instead of the responses.
    #[clap(long)]
    output_metrics: bool,
}

impl StreamCallCommand {
    pub fn run(self) -> orfail::Result<()> {
        let streams = self.connect_to_servers().or_fail()?;
        let mut input_txs = Vec::new();
        let (output_tx, output_rx) = mpsc::channel();
        for stream in streams {
            let pipelining = self.pipelining.get();
            let output_metrics = self.output_metrics;
            let (input_tx, input_rx) = mpsc::sync_channel(pipelining * 2 + 10);
            let output_tx = output_tx.clone();
            let runner = ClientRunner {
                stream,
                input_rx,
                output_tx,
                pipelining,
                output_metrics,
            };
            std::thread::spawn(move || {
                runner
                    .run()
                    .or_fail()
                    .unwrap_or_else(|e| eprintln!("Thread aborted: {}", e));
            });
            input_txs.push(input_tx);
        }

        let stdin = std::io::stdin();
        let mut input_stream = JsonlStream::new(stdin.lock());

        let stdout = std::io::stdout();
        let mut output_stream = JsonlStream::new(stdout.lock());

        let mut next_thread_index = 0;
        let mut ongoing_calls = 0;
        while let Some(mut input) = maybe_eos(input_stream.read_object()).or_fail()? {
            // Send the input.
            let mut retried_count = 0;
            loop {
                if let Err(e) = input_txs[next_thread_index].try_send(input) {
                    match e {
                        mpsc::TrySendError::Full(v) => {
                            input = v;
                            next_thread_index = (next_thread_index + 1) % input_txs.len();
                            retried_count += 1;
                            if retried_count == input_txs.len() {
                                std::thread::sleep(Duration::from_millis(10));
                                retried_count = 0;
                            }
                            continue;
                        }
                        mpsc::TrySendError::Disconnected(_) => {
                            return Err(Failure::new(format!(
                                "{next_thread_index}-th thread disconnected"
                            )));
                        }
                    }
                }

                ongoing_calls += 1;
                break;
            }
            next_thread_index = (next_thread_index + 1) % input_txs.len();

            // Receive outputs.
            while let Ok(output) = output_rx.try_recv() {
                ongoing_calls -= 1;
                output_stream.write_object(&output).or_fail()?;
            }
        }
        for input_tx in input_txs {
            std::mem::drop(input_tx);
        }

        // Receive remaining outputs.
        for _ in 0..ongoing_calls {
            let output = output_rx.recv().or_fail()?;
            output_stream.write_object(&output).or_fail()?;
        }

        Ok(())
    }

    fn connect_to_servers(&self) -> orfail::Result<Vec<JsonlStream<TcpStream>>> {
        let mut streams = Vec::new();
        for server in std::iter::once(&self.server_addr).chain(self.additional_server_addrs.iter())
        {
            let mut last_connect_error = None;
            for server_addr in server.to_socket_addrs().or_fail()? {
                match TcpStream::connect(server_addr)
                    .or_fail_with(|e| format!("Failed to connect to '{server_addr}': {e}"))
                {
                    Ok(socket) => {
                        socket.set_nodelay(true).or_fail()?;
                        streams.push(JsonlStream::new(socket));
                        break;
                    }
                    Err(error) => {
                        last_connect_error = Some(error);
                        continue;
                    }
                };
            }
            if let Some(e) = last_connect_error {
                return Err(e);
            }
        }
        Ok(streams)
    }
}

#[derive(Debug)]
struct ClientRunner {
    stream: JsonlStream<TcpStream>,
    input_rx: mpsc::Receiver<MaybeBatch<RequestObject>>,
    output_tx: mpsc::Sender<Output>,
    pipelining: usize,
    output_metrics: bool,
}

impl ClientRunner {
    fn run(self) -> orfail::Result<()> {
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
enum Output {
    Response(MaybeBatch<ResponseObject>),
    Metrics(Metrics),
}

#[derive(Debug, Serialize, Deserialize)]
struct Metrics {
    // TODO
}

fn maybe_eos<T>(result: serde_json::Result<T>) -> serde_json::Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(e) if e.io_error_kind() == Some(std::io::ErrorKind::UnexpectedEof) => Ok(None),
        Err(e) => Err(e),
    }
}
