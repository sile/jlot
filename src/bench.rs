use std::io::{BufRead, Read, Write};
use std::num::NonZeroUsize;

use orfail::OrFail;

use crate::types::{Request, Response, ServerAddr};

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

        let mut events = mio::Events::with_capacity(128);
        while !requests.is_empty() || ongoing_requests > 0 {
            while ongoing_requests < self.concurrency.get()
                && let Some(request) = requests.pop()
            {
                let (_, i) = channel_requests.pop_first().or_fail()?;
                channels[i].add_request(&mut poll, request).or_fail()?;
                channel_requests.insert((channels[i].ongoing_requests, i));
                ongoing_requests += 1;
            }

            poll.poll(&mut events, None).or_fail()?;

            for event in &events {
                let i = event.token().0;
                let channel = &mut channels[i];
                if event.is_writable() {
                    channel.send_request(&mut poll).or_fail()?;
                }
                if event.is_readable() {
                    ongoing_requests -= channel.ongoing_requests;
                    channel.recv_response().or_fail()?;
                    ongoing_requests += channel.ongoing_requests;
                }
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
    recv_buf: Vec<u8>,
    ongoing_requests: usize,
    requests: Vec<Request>,
}

impl RpcChannel {
    fn new(token: mio::Token, stream: mio::net::TcpStream) -> Self {
        Self {
            token,
            stream,
            send_buf: Vec::new(),
            send_buf_offset: 0,
            recv_buf: Vec::new(),
            ongoing_requests: 0,
            requests: Vec::new(),
        }
    }

    fn add_request(&mut self, poll: &mut mio::Poll, request: Request) -> orfail::Result<()> {
        let needs_writable = self.send_buf.is_empty();

        self.send_buf
            .extend_from_slice(request.json.value().as_raw_str().as_bytes());
        self.send_buf.push(b'\n');

        self.requests.push(request);
        self.ongoing_requests += 1;

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

    fn send_request(&mut self, poll: &mut mio::Poll) -> orfail::Result<()> {
        while self.send_buf_offset < self.send_buf.len() {
            match self.stream.write(&self.send_buf[self.send_buf_offset..]) {
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    break;
                }
                Err(e) => return Err(orfail::Failure::new(format!("failed to send request: {e}"))),
                Ok(0) => return Err(orfail::Failure::new("Connection closed by server")),
                Ok(n) => self.send_buf_offset += n,
            }
        }

        if self.send_buf_offset == self.send_buf.len() {
            self.send_buf.clear();
            self.send_buf_offset = 0;

            poll.registry()
                .reregister(&mut self.stream, self.token, mio::Interest::READABLE)
                .or_fail()?;
        }

        Ok(())
    }

    fn recv_response(&mut self) -> orfail::Result<()> {
        // Read responses
        let mut temp_buf = [0u8; 4096];
        match self.stream.read(&mut temp_buf) {
            Ok(0) => {
                return Err(orfail::Failure::new("Server closed connection"));
            }
            Ok(n) => {
                self.recv_buf.extend_from_slice(&temp_buf[..n]);

                // Parse complete lines (JSON-RPC responses)
                while let Some(newline_pos) = self
                    .recv_buf
                    //   [self    .recv_buf_offset..]
                    .iter()
                    .position(|&b| b == b'\n')
                {
                    let line_end = /*self    .recv_buf_offset +*/ newline_pos;
                    let response_line = String::from_utf8_lossy(
                        &self    .recv_buf[/*self    .recv_buf_offset*/..line_end],
                    );

                    let _response = Response::parse(response_line.to_string()).or_fail()?;
                    // Process response as needed
                    // writeln!(&mut output_writer, "{}", _response.json).or_fail()?;

                    // self    .recv_buf_offset = line_end + 1;
                    self.ongoing_requests = self.ongoing_requests.saturating_sub(1);
                }

                /*// Compact buffer if too much space is wasted
                if self    .recv_buf_offset > 4096 {
                    self    .recv_buf.drain(..self    .recv_buf_offset);
                    self    .recv_buf_offset = 0;
                }*/
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No data available, wait for next event
            }
            Err(e) => {
                return Err(orfail::Failure::new(format!("Read error: {}", e)));
            }
        }
        Ok(())
    }
}
