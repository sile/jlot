use std::{
    collections::{HashMap, VecDeque},
    net::{SocketAddr, TcpStream},
    num::NonZeroUsize,
    sync::mpsc::{self, RecvError},
    time::{Duration, Instant},
};

use jsonlrpc::{JsonlStream, MaybeBatch, RequestId, RequestObject, ResponseObject};
use orfail::{Failure, OrFail};
use serde::{Deserialize, Serialize};

use crate::io;

/// Execute a stream of JSON-RPC calls received from the standard input.
#[derive(Debug, clap::Args)]
pub struct StreamCallCommand {
    /// JSON-RPC server address or hostname.
    server_addr: String,

    /// Additional JSON-RPC servers to execute the calls in parallel.
    additional_server_addrs: Vec<String>,

    /// Maximum number of concurrent calls for each server.
    #[clap(short, long, default_value = "1")]
    pipelining: NonZeroUsize,

    /// Add metadata to each response object (note that the ID of each request will be reassigned to be unique).
    #[clap(short, long)]
    add_metadata: bool,

    /// Read the entire standard input stream before sending any requests.
    #[clap(long)]
    preread: bool,

    #[clap(long)]
    dry_run: bool,
}

impl StreamCallCommand {
    pub fn run(self) -> orfail::Result<()> {
        let streams = self.connect_to_servers().or_fail()?;
        let mut input_txs = Vec::new();
        let (output_tx, output_rx) = mpsc::channel();
        let base_time = Instant::now();
        for (server_addr, stream) in streams {
            let pipelining = self.pipelining.get();
            let (input_tx, input_rx) = mpsc::sync_channel(pipelining * 2 + 10);
            let output_tx = output_tx.clone();
            if let Some(stream) = stream {
                let runner = ClientRunner {
                    server_addr: stream.inner().peer_addr().or_fail()?,
                    stream,
                    base_time,
                    input_rx,
                    output_tx,
                    pipelining,
                    ongoing_calls: 0,
                    requests: HashMap::new(),
                };
                std::thread::spawn(move || {
                    runner
                        .run()
                        .or_fail()
                        .unwrap_or_else(|e| eprintln!("Thread aborted: {}", e));
                });
            } else {
                let runner = ClientDryRunner {
                    server_addr: server_addr.parse::<SocketAddr>().or_fail()?,
                    base_time,
                    input_rx,
                    output_tx,
                    pipelining,
                    ongoing_calls: 0,
                    responses: VecDeque::new(),
                };
                std::thread::spawn(move || {
                    runner
                        .run()
                        .or_fail()
                        .unwrap_or_else(|e| eprintln!("Thread aborted: {}", e));
                });
            }
            input_txs.push(input_tx);
        }

        let stdin = std::io::stdin();
        let mut input_stream = JsonlStream::new(stdin.lock());

        let stdout = std::io::stdout();
        let mut output_stream = JsonlStream::new(stdout.lock());

        let mut next_thread_index = 0;
        let mut ongoing_calls = 0;
        let mut next_id = 0;

        let mut inputs = Vec::new();
        if self.preread {
            while let Some(request) = io::maybe_eos(input_stream.read_value()).or_fail()? {
                inputs.push(Input::new(request));
            }
            inputs.reverse();
        }

        while let Some(mut input) = if self.preread {
            inputs.pop()
        } else {
            io::maybe_eos(input_stream.read_value())
                .or_fail()?
                .map(Input::new)
        } {
            if self.add_metadata {
                input.reassign_id(&mut next_id);
            }

            let is_notification = input.is_notification;

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

                if !is_notification {
                    ongoing_calls += 1;
                }
                break;
            }
            next_thread_index = (next_thread_index + 1) % input_txs.len();

            // Receive outputs.
            while let Ok(output) = output_rx.try_recv() {
                ongoing_calls -= 1;
                output_stream.write_value(&output).or_fail()?;
            }
        }
        for input_tx in input_txs {
            std::mem::drop(input_tx);
        }

        // Receive remaining outputs.
        for _ in 0..ongoing_calls {
            let output = output_rx.recv().or_fail()?;
            output_stream.write_value(&output).or_fail()?;
        }

        Ok(())
    }

    fn connect_to_servers(&self) -> orfail::Result<Vec<(&String, Option<JsonlStream<TcpStream>>)>> {
        let mut streams = Vec::new();
        for server in std::iter::once(&self.server_addr).chain(self.additional_server_addrs.iter())
        {
            if self.dry_run {
                streams.push((server, None));
            } else {
                let socket = TcpStream::connect(server)
                    .or_fail_with(|e| format!("Failed to connect to '{server}': {e}"))?;
                socket.set_nodelay(true).or_fail()?;
                streams.push((server, Some(JsonlStream::new(socket))));
            }
        }
        Ok(streams)
    }
}

#[derive(Debug)]
struct ClientRunner {
    stream: JsonlStream<TcpStream>,
    server_addr: SocketAddr,
    base_time: Instant,
    input_rx: mpsc::Receiver<Input>,
    output_tx: mpsc::Sender<Output>,
    pipelining: usize,
    ongoing_calls: usize,
    requests: HashMap<RequestId, Metadata>,
}

impl ClientRunner {
    fn run(mut self) -> orfail::Result<()> {
        while self.run_one().or_fail()? {}
        Ok(())
    }

    fn run_one(&mut self) -> orfail::Result<bool> {
        while self.ongoing_calls < self.pipelining {
            match self.input_rx.recv() {
                Ok(input) => {
                    self.send_request(input).or_fail()?;
                }
                Err(RecvError) => {
                    if self.ongoing_calls == 0 {
                        return Ok(false);
                    }
                    break;
                }
            }
        }

        self.recv_response().or_fail()?;
        Ok(true)
    }

    fn send_request(&mut self, input: Input) -> orfail::Result<()> {
        let is_notification = input.is_notification;

        let start_time = self.base_time.elapsed();
        self.stream.write_value(&input.request).or_fail()?;
        if !is_notification {
            self.ongoing_calls += 1;

            if let Some(id) = input.metadata_id {
                let metadata = Metadata {
                    request: input.request,
                    server: self.server_addr,
                    start_time,
                    end_time: Duration::default(),
                };
                self.requests.insert(id, metadata);
            }
        }
        Ok(())
    }

    fn recv_response(&mut self) -> orfail::Result<()> {
        let mut response: MaybeBatch<ResponseWithMetadata> = self.stream.read_value().or_fail()?;

        let metadata = if self.requests.is_empty() {
            None
        } else {
            response
                .iter()
                .find_map(|r| r.response.id())
                .and_then(|id| self.requests.remove(id))
        };

        if let Some(mut metadata) = metadata {
            metadata.end_time = self.base_time.elapsed();
            if let Some(r) = response.iter_mut().next() {
                r.metadata = Some(metadata);
            }
        }

        self.output_tx.send(response).or_fail()?;
        self.ongoing_calls -= 1;
        Ok(())
    }
}

#[derive(Debug)]
struct Input {
    request: MaybeBatch<RequestObject>,
    is_notification: bool,
    metadata_id: Option<RequestId>,
}

impl Input {
    fn new(request: MaybeBatch<RequestObject>) -> Self {
        let is_notification = request.iter().all(|r| r.id.is_none());
        Self {
            request,
            is_notification,
            metadata_id: None,
        }
    }

    fn reassign_id(&mut self, next_id: &mut i64) {
        if self.is_notification {
            return;
        }

        for r in self.request.iter_mut().filter(|r| r.id.is_some()) {
            r.id = Some(RequestId::Number(*next_id));
            if self.metadata_id.is_none() {
                self.metadata_id = r.id.clone();
            }
            *next_id += 1;
        }
    }
}

pub type Output = MaybeBatch<ResponseWithMetadata>;

#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseWithMetadata {
    #[serde(flatten)]
    pub response: ResponseObject,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Metadata>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Metadata {
    pub request: MaybeBatch<RequestObject>,
    pub server: SocketAddr,
    pub start_time: Duration,
    pub end_time: Duration,
}

#[derive(Debug)]
struct ClientDryRunner {
    server_addr: SocketAddr,
    base_time: Instant,
    input_rx: mpsc::Receiver<Input>,
    output_tx: mpsc::Sender<Output>,
    pipelining: usize,
    ongoing_calls: usize,
    responses: VecDeque<ResponseWithMetadata>,
}

impl ClientDryRunner {
    fn run(mut self) -> orfail::Result<()> {
        while self.run_one().or_fail()? {}
        Ok(())
    }

    fn run_one(&mut self) -> orfail::Result<bool> {
        while self.ongoing_calls < self.pipelining {
            match self.input_rx.recv() {
                Ok(input) => {
                    self.send_request(input);
                }
                Err(RecvError) => {
                    if self.ongoing_calls == 0 {
                        return Ok(false);
                    }
                    break;
                }
            }
        }

        self.recv_response().or_fail()?;
        Ok(true)
    }

    fn send_request(&mut self, input: Input) {
        let is_notification = input.is_notification;

        let start_time = self.base_time.elapsed();
        if !is_notification {
            self.ongoing_calls += 1;

            let mut response = ResponseWithMetadata {
                response: ResponseObject::Ok {
                    jsonrpc: jsonlrpc::JsonRpcVersion::V2,
                    id: input
                        .request
                        .iter()
                        .next()
                        .and_then(|r| r.id.clone())
                        .or_fail()
                        .expect("unreachable"),
                    result: serde_json::Value::Null,
                },
                metadata: None,
            };

            if input.metadata_id.is_some() {
                let metadata = Metadata {
                    request: input.request,
                    server: self.server_addr,
                    start_time,
                    end_time: Duration::default(),
                };
                response.metadata = Some(metadata);
            }

            self.responses.push_back(response);
        }
    }

    fn recv_response(&mut self) -> orfail::Result<()> {
        let mut response = self.responses.pop_front().or_fail()?;
        if let Some(metadata) = &mut response.metadata {
            metadata.end_time = self.base_time.elapsed();
        }
        self.output_tx
            .send(MaybeBatch::Single(response))
            .or_fail()?;
        self.ongoing_calls -= 1;
        Ok(())
    }
}
