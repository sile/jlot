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
        poll: mio::Poll::new().or_fail()?,
        channels: Vec::new(),
        requests: Vec::new(),
        ongoing_requests: 0,
        channel_requests: std::collections::BTreeSet::new(),
        base_time: std::time::Instant::now(),
        base_unix_timestamp: std::time::Duration::ZERO,
    };
    command.run().or_fail()?;

    Ok(true)
}

struct BenchCommand {
    server_addrs: Vec<ServerAddr>,
    concurrency: NonZeroUsize,
    poll: mio::Poll,
    channels: Vec<RpcChannel>,
    requests: Vec<Request>,
    ongoing_requests: usize,
    channel_requests: std::collections::BTreeSet<(usize, usize)>,
    base_time: std::time::Instant,
    base_unix_timestamp: std::time::Duration,
}

impl BenchCommand {
    fn run(mut self) -> orfail::Result<()> {
        self.setup_rpc_channels().or_fail()?;
        self.read_requests().or_fail()?;
        self.run_rpc_calls().or_fail()?;

        let stdout = std::io::stdout();
        let mut output_writer = std::io::BufWriter::new(stdout.lock());

        for channel in self.channels {
            let mut requests = channel
                .requests
                .into_iter()
                .zip(channel.start_times)
                .map(|(mut r, t)| (r.id.take(), (r, t)))
                .collect::<std::collections::HashMap<_, _>>();
            for (line, end_time) in std::io::BufReader::new(&channel.recv_buf[..])
                .lines()
                .zip(channel.end_times)
            {
                let line = line.or_fail()?;
                let mut response = Response::parse(line).or_fail()?;
                let id = response.id.take().or_fail_with(|()| "TODO".to_owned())?;
                let (request, start_time) = requests
                    .remove(&Some(id))
                    .or_fail_with(|()| "TODO".to_owned())?;
                let start_unix_timestamp =
                    start_time.duration_since(self.base_time) + self.base_unix_timestamp;
                let end_unix_timestamp =
                    end_time.duration_since(self.base_time) + self.base_unix_timestamp;
                writeln!(
                    output_writer,
                    "{}",
                    nojson::object(|f| {
                        for (name, value) in request.json.value().to_object().expect("bug") {
                            let name = name.to_unquoted_string_str().expect("infallibe");
                            f.member(name, value)?;
                        }
                        for (name, value) in response.json.value().to_object().expect("bug") {
                            let name = name.to_unquoted_string_str().expect("infallibe");
                            if !matches!(name.as_ref(), "jsonrpc" | "id") {
                                f.member(name, value)?;
                            }
                        }
                        f.member("server", &channel.server_addr.0)?;
                        f.member("request_byte_size", request.json.text().len())?;
                        f.member("response_byte_size", response.json.text().len())?;
                        f.member(
                            "start_unix_timestamp_micros",
                            start_unix_timestamp.as_micros(),
                        )?;
                        f.member("end_unix_timestamp_micros", end_unix_timestamp.as_micros())?;
                        Ok(())
                    })
                )
                .or_fail()?;
            }
        }

        output_writer.flush().or_fail()?;

        Ok(())
    }

    fn run_rpc_calls(&mut self) -> orfail::Result<()> {
        self.base_time = std::time::Instant::now();
        self.base_unix_timestamp = std::time::UNIX_EPOCH.elapsed().or_fail()?;

        let mut events = mio::Events::with_capacity(self.channels.len());
        while !self.requests.is_empty() || self.ongoing_requests > 0 {
            if self.ongoing_requests < self.concurrency.get() {
                self.enqueue_pending_requests().or_fail()?;
            }

            self.poll.poll(&mut events, None).or_fail()?;

            for event in &events {
                let i = event.token().0;
                let channel = &mut self.channels[i];
                if event.is_writable() {
                    channel.send_request(&mut self.poll).or_fail()?;
                }
                if event.is_readable() {
                    let old_count = channel.ongoing_requests;
                    self.ongoing_requests -= old_count;
                    channel.recv_response().or_fail()?;
                    self.ongoing_requests += channel.ongoing_requests;

                    self.channel_requests.remove(&(old_count, i));
                    self.channel_requests.insert((channel.ongoing_requests, i));
                }
            }
        }

        Ok(())
    }

    fn enqueue_pending_requests(&mut self) -> orfail::Result<()> {
        let now = std::time::Instant::now();
        while self.ongoing_requests < self.concurrency.get()
            && let Some(request) = self.requests.pop()
        {
            let (_, i) = self.channel_requests.pop_first().or_fail()?;
            self.channels[i]
                .enqueue_request(&mut self.poll, now, request)
                .or_fail()?;
            self.channel_requests
                .insert((self.channels[i].ongoing_requests, i));
            self.ongoing_requests += 1;
        }
        Ok(())
    }

    fn setup_rpc_channels(&mut self) -> orfail::Result<()> {
        for (i, server_addr) in self.server_addrs.iter().enumerate() {
            let addr = &server_addr.0;
            let stream = std::net::TcpStream::connect(addr)
                .or_fail_with(|e| format!("Failed to connect to '{addr}': {e}"))?;
            stream.set_nodelay(true).or_fail()?;
            stream.set_nonblocking(true).or_fail()?;

            let token = mio::Token(i);
            let mut stream = mio::net::TcpStream::from_std(stream);
            self.poll
                .registry()
                .register(&mut stream, token, mio::Interest::READABLE)
                .or_fail()?;

            self.channels
                .push(RpcChannel::new(token, server_addr.clone(), stream));
        }

        self.channel_requests = std::collections::BTreeSet::new();
        for i in 0..self.channels.len() {
            self.channel_requests.insert((0, i));
        }

        Ok(())
    }

    fn read_requests(&mut self) -> orfail::Result<()> {
        let stdin = std::io::stdin();
        let input_reader = std::io::BufReader::new(stdin.lock());
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

            self.requests.push(request);
        }

        self.requests.reverse();

        Ok(())
    }
}

struct RpcChannel {
    token: mio::Token,
    server_addr: ServerAddr,
    stream: mio::net::TcpStream,
    send_buf: Vec<u8>,
    send_buf_offset: usize,
    recv_buf: Vec<u8>,
    ongoing_requests: usize,
    requests: Vec<Request>,
    start_times: Vec<std::time::Instant>,
    end_times: Vec<std::time::Instant>,
}

impl RpcChannel {
    fn new(token: mio::Token, server_addr: ServerAddr, stream: mio::net::TcpStream) -> Self {
        Self {
            token,
            server_addr,
            stream,
            send_buf: Vec::new(),
            send_buf_offset: 0,
            recv_buf: Vec::new(),
            ongoing_requests: 0,
            requests: Vec::new(),
            start_times: Vec::new(),
            end_times: Vec::new(),
        }
    }

    fn enqueue_request(
        &mut self,
        poll: &mut mio::Poll,
        now: std::time::Instant,
        request: Request,
    ) -> orfail::Result<()> {
        let needs_writable = self.send_buf.is_empty();

        self.send_buf
            .extend_from_slice(request.json.value().as_raw_str().as_bytes());
        self.send_buf.push(b'\n');

        self.start_times.push(now);
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
        let mut buf = [0; 4096];
        loop {
            let n = match self.stream.read(&mut buf) {
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(orfail::Failure::new(format!("Read error: {e}"))),
                Ok(0) => return Err(orfail::Failure::new("Server closed connection")),
                Ok(n) => n,
            };

            let count = buf[..n].iter().filter(|&&b| b == b'\n').count();
            if count > 0 {
                let now = std::time::Instant::now();
                self.end_times.extend(std::iter::repeat_n(now, count));
                self.ongoing_requests = self
                    .ongoing_requests
                    .checked_sub(count)
                    .or_fail_with(|()| "too many responses".to_owned())?;
            }

            self.recv_buf.extend_from_slice(&buf[..n]);
        }
        Ok(())
    }
}
