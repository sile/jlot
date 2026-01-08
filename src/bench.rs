use std::io::{BufRead, Write};
use std::num::NonZeroUsize;

use orfail::OrFail;

use crate::types::{Request, ServerAddr};

pub fn try_run(args: &mut noargs::RawArgs) -> noargs::Result<bool> {
    if !noargs::cmd("bench").doc("TODO").take(args).is_present() {
        return Ok(false);
    }

    let concurrency: NonZeroUsize = noargs::opt("concurrency")
        .short('c')
        .ty("INTEGER")
        .doc("Number of concurrent requests")
        .default("1")
        .take(args)
        .then(|o| o.value().parse())?;

    let server_addr_arg = noargs::arg("<SERVER>...")
        .doc("JSON-RPC server address or hostname")
        .example("127.0.0.1:8080");
    let mut server_addrs: Vec<ServerAddr> = Vec::new();
    server_addrs.push(server_addr_arg.take(args).then(|a| a.value().parse())?);
    while let Some(addr) = server_addr_arg
        .take(args)
        .present_and_then(|a| a.value().parse())?
    {
        server_addrs.push(addr);
    }

    if args.metadata().help_mode {
        return Ok(false);
    }

    let command = BenchCommand {
        server_addrs,
        concurrency,
    };
    command.run().or_fail()?;

    Ok(true)
}

struct BenchCommand {
    server_addrs: Vec<ServerAddr>,
    concurrency: NonZeroUsize,
}

impl BenchCommand {
    fn run(self) -> orfail::Result<()> {
        let mut poll = mio::Poll::new().or_fail()?;
        let channels = self.connect_to_servers(&mut poll).or_fail()?;
        let requests = self.read_requests().or_fail()?;

        /*
        writeln!(rpc_writer, "{}", request.json).or_fail()?;
        rpc_writer.flush().or_fail()?;

        if request.id.is_some() {
            let mut response_line = String::new();
            let bytes_read = rpc_reader.read_line(&mut response_line).or_fail()?;
            (bytes_read > 0).or_fail_with(|()| {
                "Faied to receive RPC response: unexpected EOF".to_owned()
            })?;

            let response = Response::parse(response_line).or_fail()?;
            writeln!(output_writer, "{}", response.json).or_fail()?;
        }
        */

        let stdout = std::io::stdout();
        let mut output_writer = std::io::BufWriter::new(stdout.lock());
        output_writer.flush().or_fail()?;

        Ok(())
    }

    fn connect_to_servers(&self, poll: &mut mio::Poll) -> orfail::Result<Vec<RpcChannel>> {
        self.server_addrs
            .iter()
            .enumerate()
            .map(|(i, addr)| {
                let addr = &addr.0;
                let stream = std::net::TcpStream::connect(addr)
                    .or_fail_with(|e| format!("Failed to connect to '{addr}': {e}"))?;

                stream.set_nodelay(true).or_fail()?;
                stream.set_nonblocking(true).or_fail()?;

                let token = mio::Token(i);
                let mut stream0 = mio::net::TcpStream::from_std(stream.try_clone().or_fail()?);
                let stream1 = mio::net::TcpStream::from_std(stream);
                poll.registry()
                    .register(
                        &mut stream0,
                        token,
                        mio::Interest::READABLE | mio::Interest::WRITABLE,
                    )
                    .or_fail()?;

                Ok(RpcChannel {
                    token,
                    writer: std::io::BufWriter::new(stream0),
                    reader: std::io::BufReader::new(stream1),
                })
            })
            .collect()
    }

    fn read_requests(&self) -> orfail::Result<Vec<Request>> {
        let stdin = std::io::stdin();
        let input_reader = std::io::BufReader::new(stdin.lock());
        let mut requests = Vec::new();
        let mut ids = std::collections::HashSet::new();

        for line in input_reader.lines() {
            let line = line.or_fail()?;
            let request = Request::parse(line).or_fail()?;

            if let Some(id) = &request.id {
                (!ids.contains(id)).or_fail_with(|()| {
                    format!("request contains duplicate ID: {}", request.json)
                })?;
                ids.insert(id.clone());
            }
            requests.push(request);
        }
        Ok(requests)
    }
}

struct RpcChannel {
    token: mio::Token,
    reader: std::io::BufReader<mio::net::TcpStream>,
    writer: std::io::BufWriter<mio::net::TcpStream>,
}
