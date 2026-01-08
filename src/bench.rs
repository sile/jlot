use std::io::{BufRead, Write};
use std::num::NonZeroUsize;

use orfail::OrFail;

use crate::types::{Request, RequestId, ServerAddr};

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
        let mut channels = self.connect_to_servers(&mut poll).or_fail()?;
        let mut requests = self.read_requests().or_fail()?;
        requests.reverse();

        let mut ongoing_requests = 0;
        let mut channel_requests = std::collections::BTreeSet::new();
        for i in 0..channels.len() {
            channel_requests.insert((0, i));
        }

        while !requests.is_empty() || ongoing_requests > 0 {
            while ongoing_requests < self.concurrency.get()
                && let Some(request) = requests.pop()
            {
                let (count, i) = channel_requests.pop_first().or_fail()?;
                channels[i].add_request(&mut poll, request).or_fail()?;
                channel_requests.insert((count + 1, i));
                ongoing_requests += 1;
            }
        }

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
                let mut stream = mio::net::TcpStream::from_std(stream);
                poll.registry()
                    .register(&mut stream, token, mio::Interest::READABLE)
                    .or_fail()?;

                Ok(RpcChannel::new(token, stream))
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

            let id = request.id.as_ref().or_fail_with(|()| {
                format!(
                    "bench command does not support notificaion: {}",
                    request.json
                )
            })?;
            (!ids.contains(id))
                .or_fail_with(|()| format!("request contains duplicate ID: {}", request.json))?;
            ids.insert(id.clone());

            requests.push(request);
        }
        Ok(requests)
    }
}

struct RpcChannel {
    token: mio::Token,
    stream: mio::net::TcpStream,
    send_buf: Vec<u8>,
    send_buf_offset: usize,
    requests: std::collections::HashMap<RequestId, Request>,
}

impl RpcChannel {
    fn new(token: mio::Token, stream: mio::net::TcpStream) -> Self {
        Self {
            token,
            stream,
            send_buf: Vec::new(),
            send_buf_offset: 0,
            requests: std::collections::HashMap::new(),
        }
    }

    fn add_request(&mut self, poll: &mut mio::Poll, request: Request) -> orfail::Result<()> {
        let needs_writable = self.send_buf.is_empty();

        self.send_buf
            .extend_from_slice(request.json.value().as_raw_str().as_bytes());
        self.send_buf.push(b'\n');

        self.requests.insert(request.id.clone().or_fail()?, request);

        if needs_writable {
            poll.registry()
                .reregister(
                    &mut self.stream,
                    self.token,
                    mio::Interest::READABLE | mio::Interest::WRITABLE,
                )
                .or_fail()?;
        }

        Ok(())
    }
}
