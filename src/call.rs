use std::io::{BufRead, Write};
use std::net::TcpStream;

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
        let stdin = std::io::stdin();
        let reader = std::io::BufReader::new(stdin.lock());
        let stdout = std::io::stdout();
        let mut writer = std::io::BufWriter::new(stdout.lock());

        let mut runner = ClientRunner {
            writer: std::io::BufWriter::new(stream.try_clone().or_fail()?),
            reader: std::io::BufReader::new(stream),
        };

        for line in reader.lines() {
            let line = line.or_fail()?;
            let request = Request::parse(line).or_fail()?;
            let input = Input::new(request);

            runner.send_request(&input).or_fail()?;

            if !input.is_notification {
                let response = runner.recv_response().or_fail()?;
                writeln!(writer, "{}", response.json).or_fail()?;
            }
        }

        writer.flush().or_fail()?;
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
}

impl ClientRunner {
    fn send_request(&mut self, input: &Input) -> orfail::Result<()> {
        writeln!(self.writer, "{}", input.request.json).or_fail()?;
        self.writer.flush().or_fail()?;
        Ok(())
    }

    fn recv_response(&mut self) -> orfail::Result<Response> {
        let mut response_line = String::new();
        self.reader.read_line(&mut response_line).or_fail()?;
        Response::parse(response_line).or_fail()
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
