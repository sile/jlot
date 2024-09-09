use std::{
    net::{TcpStream, ToSocketAddrs},
    num::NonZeroUsize,
    sync::mpsc,
};

use jsonlrpc::{JsonlStream, MaybeBatch, RequestObject, ResponseObject, RpcClient};
use orfail::{Failure, OrFail};

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
        let input_stream = JsonlStream::new(stdin.lock());
        todo!()
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

#[derive(Debug)]
enum Output {
    Response(MaybeBatch<ResponseObject>),
    Metrics(Metrics),
}

#[derive(Debug)]
struct Metrics {
    // TODO
}
