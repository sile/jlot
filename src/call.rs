use std::{
    collections::{HashMap, VecDeque},
    io::{BufRead, Write},
    net::{SocketAddr, TcpStream},
    num::NonZeroUsize,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use jsonlrpc::{JsonlStream, RequestId, ResponseObject};
use orfail::OrFail;

use crate::types::{Request, ServerAddr};

pub fn try_run(args: &mut noargs::RawArgs) -> noargs::Result<bool> {
    if !noargs::cmd("call")
        .doc("Read JSON-RPC requests from standard input and execute the RPC calls")
        .take(args)
        .is_present()
    {
        return Ok(false);
    }

    let server_addr: ServerAddr = noargs::arg("<SERVER>")
        .doc("JSON-RPC server address or hostname")
        .example("127.0.0.1:8080")
        .take(args)
        .then(|a| a.value().parse())?;
    let additional_server_addrs: Vec<ServerAddr> = {
        let mut addrs = Vec::new();
        loop {
            let result = noargs::arg("[SERVER]...")
                .doc("Additional JSON-RPC servers to execute calls in parallel")
                .take(args)
                .present_and_then(|a| a.value().parse())?;

            match result {
                Some(addr) => addrs.push(addr),
                None => break,
            }
        }
        addrs
    };
    let concurrency: NonZeroUsize = noargs::opt("concurrency")
        .short('c')
        .ty("NUMBER")
        .doc("Maximum number of concurrent calls")
        .default("1")
        .take(args)
        .then(|o| o.value().parse())?;
    let add_metadata: bool = noargs::flag("add-metadata")
        .short('m')
        .doc("Add metadata to each response object (note that the ID of each request will be reassigned to be unique)")
        .take(args)
        .is_present();
    let dry_run: bool = noargs::flag("dry-run")
        .doc(concat!(
            "Run the command without connecting to or communicating with actual servers\n",
            "\n",
            "All RPC responses will be set to `null`"
        ))
        .take(args)
        .is_present();

    if args.metadata().help_mode {
        return Ok(false);
    }

    run_call(
        server_addr,
        additional_server_addrs,
        concurrency,
        add_metadata,
        dry_run,
    )?;

    Ok(true)
}

fn run_call(
    server_addr: ServerAddr,
    additional_server_addrs: Vec<ServerAddr>,
    concurrency: NonZeroUsize,
    add_metadata: bool,
    dry_run: bool,
) -> orfail::Result<()> {
    let call_command = CallCommand {
        server_addr,
        additional_server_addrs,
        concurrency,
        add_metadata,
        dry_run,
    };
    call_command.run()
}

struct CallCommand {
    server_addr: ServerAddr,
    additional_server_addrs: Vec<ServerAddr>,
    concurrency: NonZeroUsize,
    add_metadata: bool,
    dry_run: bool,
}

impl CallCommand {
    fn run(self) -> orfail::Result<()> {
        let streams = self.connect_to_servers().or_fail()?;
        let (output_tx, output_rx) = mpsc::channel();

        let output_thread = std::thread::spawn(move || {
            let stdout = std::io::stdout();
            let mut writer = std::io::BufWriter::new(stdout.lock());
            while let Ok(output) = output_rx.recv() {
                let _ = writeln!(writer, "{}", nojson::Json(output));
            }
        });

        let stdin = std::io::stdin();
        let reader = std::io::BufReader::new(stdin.lock());
        let mut inputs = Vec::new();
        let mut next_id = 0;
        for line in reader.lines() {
            let line = line.or_fail()?;
            let request = Request::parse(line).or_fail()?;
            let mut input = Input::new(request);
            if self.add_metadata {
                input.reassign_id(&mut next_id);
            }
            inputs.push(input);
        }

        let inputs = Arc::new(inputs);
        let input_index = Arc::new(AtomicUsize::new(0));

        let base_time = Instant::now();
        for ((server_addr, stream), pipelining) in
            self.servers().zip(streams).zip(self.pipelinings())
        {
            let output_tx = output_tx.clone();
            if let Some(stream) = stream {
                let runner = ClientRunner {
                    server_addr: stream.inner().peer_addr().or_fail()?,
                    writer: std::io::BufWriter::new(stream.inner().try_clone().or_fail()?),
                    stream,
                    base_time,
                    inputs: inputs.clone(),
                    input_index: input_index.clone(),
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
                    server_addr: server_addr.0.parse::<SocketAddr>().or_fail()?,
                    base_time,
                    inputs: inputs.clone(),
                    input_index: input_index.clone(),
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
        }

        std::mem::drop(output_tx);
        let _ = output_thread.join();

        Ok(())
    }

    fn connect_to_servers(&self) -> orfail::Result<Vec<Option<JsonlStream<TcpStream>>>> {
        let mut streams = Vec::new();
        for server in self.servers() {
            if self.dry_run {
                streams.push(None);
            } else {
                let socket = TcpStream::connect(&server.0)
                    .or_fail_with(|e| format!("Failed to connect to '{}': {e}", server.0))?;
                socket.set_nodelay(true).or_fail()?;
                streams.push(Some(JsonlStream::new(socket)));
            }
        }
        Ok(streams)
    }

    fn servers(&self) -> impl '_ + Iterator<Item = &ServerAddr> {
        std::iter::once(&self.server_addr).chain(self.additional_server_addrs.iter())
    }

    fn pipelinings(&self) -> impl Iterator<Item = usize> {
        let servers = 1 + self.additional_server_addrs.len();
        let pipelining = self.concurrency.get() / servers;
        let mut remainings = self.concurrency.get() % servers;
        (0..servers)
            .map(move |_| {
                if remainings > 0 {
                    remainings -= 1;
                    pipelining + 1
                } else {
                    pipelining
                }
            })
            .take_while(|pipelining| *pipelining > 0)
    }
}

struct ClientRunner {
    stream: JsonlStream<TcpStream>,
    writer: std::io::BufWriter<TcpStream>,
    server_addr: SocketAddr,
    base_time: Instant,
    inputs: Arc<Vec<Input>>,
    input_index: Arc<AtomicUsize>,
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
            let i = self.input_index.fetch_add(1, Ordering::SeqCst);
            if i < self.inputs.len() {
                self.send_request(self.inputs[i].clone()).or_fail()?;
            } else if self.ongoing_calls == 0 {
                return Ok(false);
            } else {
                break;
            }
        }
        self.recv_response().or_fail()?;
        Ok(true)
    }

    fn send_request(&mut self, input: Input) -> orfail::Result<()> {
        let is_notification = input.is_notification;

        let start_time = self.base_time.elapsed();
        writeln!(self.writer, "{}", input.request.json).or_fail()?;
        self.writer.flush().or_fail()?;

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
        let mut response: ResponseWithMetadata = self.stream.read_value().or_fail()?;

        let metadata = if self.requests.is_empty() {
            None
        } else if let Some(id) = response.response.id() {
            self.requests.remove(id)
        } else {
            None
        };

        if let Some(mut metadata) = metadata {
            metadata.end_time = self.base_time.elapsed();
            response.metadata = Some(metadata);
        }

        self.output_tx.send(response).or_fail()?;
        self.ongoing_calls -= 1;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct Input {
    request: Request,
    is_notification: bool,
    metadata_id: Option<RequestId>,
}

impl Input {
    fn new(request: Request) -> Self {
        let is_notification = request.id.is_none();
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

        self.request.id = Some(RequestId::Number(*next_id));
        if self.metadata_id.is_none() {
            self.metadata_id = self.request.id.clone();
        }
        *next_id += 1;
    }
}

pub type Output = ResponseWithMetadata;

#[derive(Debug)]
pub struct ResponseWithMetadata {
    pub response: ResponseObject,
    pub metadata: Option<Metadata>,
}

impl nojson::DisplayJson for ResponseWithMetadata {
    fn fmt(&self, f: &mut nojson::JsonFormatter<'_, '_>) -> std::fmt::Result {
        f.object(|f| {
            // TODO: f.member("response", nojson::Json(&self.response))?;

            if let Some(metadata) = &self.metadata {
                f.member(
                    "metadata",
                    nojson::object(|f| {
                        f.member("request", nojson::Json(&metadata.request))?;
                        f.member("server", &metadata.server.0)?;
                        f.member("start_time_us", metadata.start_time.as_micros())?;
                        f.member("end_time_us", metadata.end_time.as_micros())?;
                    }),
                )?;
            }

            Ok(())
        })
    }
}

#[derive(Debug)]
pub struct Metadata {
    pub request: Request,
    pub server: SocketAddr,
    pub start_time: Duration,
    pub end_time: Duration,
}

struct ClientDryRunner {
    server_addr: SocketAddr,
    base_time: Instant,
    inputs: Arc<Vec<Input>>,
    input_index: Arc<AtomicUsize>,
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
            let i = self.input_index.fetch_add(1, Ordering::SeqCst);
            if i < self.inputs.len() {
                self.send_request(self.inputs[i].clone());
            } else if self.ongoing_calls == 0 {
                return Ok(false);
            } else {
                break;
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
                    id: input.request.id.clone().or_fail().expect("unreachable"),
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
        self.output_tx.send(response).or_fail()?;
        self.ongoing_calls -= 1;
        Ok(())
    }
}
