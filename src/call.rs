use std::{
    io::{BufRead, Write},
    net::TcpStream,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
};

use orfail::OrFail;

use crate::types::{Request, Response, ServerAddr};

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

    if args.metadata().help_mode {
        return Ok(false);
    }

    let call_command = CallCommand { server_addr };
    call_command.run().or_fail()?;

    Ok(true)
}

struct CallCommand {
    server_addr: ServerAddr,
}

impl CallCommand {
    fn run(self) -> orfail::Result<()> {
        let stream = self.connect_to_server().or_fail()?;
        let (output_tx, output_rx) = mpsc::channel::<Response>();

        let output_thread = std::thread::spawn(move || {
            let stdout = std::io::stdout();
            let mut writer = std::io::BufWriter::new(stdout.lock());
            while let Ok(output) = output_rx.recv() {
                let _ = writeln!(writer, "{}", output.json);
            }
        });

        let stdin = std::io::stdin();
        let reader = std::io::BufReader::new(stdin.lock());
        let mut inputs = Vec::new();

        for line in reader.lines() {
            let line = line.or_fail()?;
            let request = Request::parse(line).or_fail()?;
            let input = Input::new(request);
            inputs.push(input);
        }

        let inputs = Arc::new(inputs);
        let input_index = Arc::new(AtomicUsize::new(0));

        let output_tx = output_tx.clone();
        let runner = ClientRunner {
            writer: std::io::BufWriter::new(stream.try_clone().or_fail()?),
            reader: std::io::BufReader::new(stream),

            inputs: inputs.clone(),
            input_index: input_index.clone(),
            output_tx,
            ongoing_calls: 0,
        };
        std::thread::spawn(move || {
            runner
                .run()
                .or_fail()
                .unwrap_or_else(|e| eprintln!("Thread aborted: {}", e));
        });

        let _ = output_thread.join();

        Ok(())
    }

    fn connect_to_server(&self) -> orfail::Result<TcpStream> {
        let stream = TcpStream::connect(&self.server_addr.0)
            .or_fail_with(|e| format!("Failed to connect to '{}': {e}", self.server_addr.0))?;
        stream.set_nodelay(true).or_fail()?;
        Ok(stream)
    }
}

struct ClientRunner {
    writer: std::io::BufWriter<TcpStream>,
    reader: std::io::BufReader<TcpStream>,
    inputs: Arc<Vec<Input>>,
    input_index: Arc<AtomicUsize>,
    output_tx: mpsc::Sender<Response>,
    ongoing_calls: usize,
}

impl ClientRunner {
    fn run(mut self) -> orfail::Result<()> {
        while self.run_one().or_fail()? {}
        Ok(())
    }

    fn run_one(&mut self) -> orfail::Result<bool> {
        while self.ongoing_calls < 1 {
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

        writeln!(self.writer, "{}", input.request.json).or_fail()?;
        self.writer.flush().or_fail()?;

        if !is_notification {
            self.ongoing_calls += 1;
        }
        Ok(())
    }

    fn recv_response(&mut self) -> orfail::Result<()> {
        let mut response_line = String::new();
        self.reader.read_line(&mut response_line).or_fail()?;
        let response = Response::parse(response_line).or_fail()?;
        self.output_tx.send(response).or_fail()?;
        self.ongoing_calls -= 1;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct Input {
    request: Request,
    is_notification: bool,
}

impl Input {
    fn new(request: Request) -> Self {
        let is_notification = request.id.is_none();
        Self {
            request,
            is_notification,
        }
    }
}
