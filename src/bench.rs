#[cfg(target_os = "linux")]
mod linux {
    use std::collections::{BTreeSet, HashSet, VecDeque};
    use std::io::{BufRead, Write};
    use std::num::NonZeroUsize;
    use std::os::fd::AsRawFd;
    use std::time::{Duration, Instant};

    use io_uring::{opcode, squeue::Entry, types, IoUring};
    use orfail::OrFail;

    use crate::types::{Request, Response, ServerAddr};

    const OP_READ: u64 = 0;
    const OP_WRITE: u64 = 1;

    pub fn try_run(args: &mut noargs::RawArgs) -> noargs::Result<bool> {
        if !noargs::cmd("bench")
            .doc("Run JSON-RPC benchmark")
            .take(args)
            .is_present()
        {
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
            return Ok(true);
        }

        let command = BenchCommand {
            server_addrs,
            concurrency,
            channels: Vec::new(),
            requests: Vec::new(),
            ongoing_requests: 0,
            channel_requests: BTreeSet::new(),
            base_time: Instant::now(),
            base_unix_timestamp: Duration::ZERO,
        };
        command.run().or_fail()?;

        Ok(true)
    }

    struct BenchCommand {
        server_addrs: Vec<ServerAddr>,
        concurrency: NonZeroUsize,
        channels: Vec<RpcChannel>,
        requests: Vec<Request>,
        ongoing_requests: usize,
        channel_requests: BTreeSet<(usize, usize)>,
        base_time: Instant,
        base_unix_timestamp: Duration,
    }

    impl BenchCommand {
        fn run(mut self) -> orfail::Result<()> {
            self.setup_rpc_channels().or_fail()?;
            self.read_requests().or_fail()?;
            self.run_rpc_calls().or_fail()?;
            self.output_results().or_fail()?;
            Ok(())
        }

        fn setup_rpc_channels(&mut self) -> orfail::Result<()> {
            for (i, server_addr) in self.server_addrs.iter().enumerate() {
                let addr = &server_addr.0;
                let stream = std::net::TcpStream::connect(addr)
                    .or_fail_with(|e| format!("Failed to connect to '{addr}': {e}"))?;
                stream.set_nodelay(true).or_fail()?;

                self.channels
                    .push(RpcChannel::new(i, server_addr.clone(), stream));
            }

            self.channel_requests = BTreeSet::new();
            for i in 0..self.channels.len() {
                self.channel_requests.insert((0, i));
            }

            Ok(())
        }

        fn read_requests(&mut self) -> orfail::Result<()> {
            let stdin = std::io::stdin();
            let input_reader = std::io::BufReader::new(stdin.lock());
            let mut ids = HashSet::new();

            for line in input_reader.lines() {
                let line = line.or_fail()?;
                let request = Request::parse(line).or_fail()?;

                let id = request.id.as_ref().or_fail_with(|()| {
                    format!(
                        "bench command does not support notification: {}",
                        request.json
                    )
                })?;
                (!ids.contains(id))
                    .or_fail_with(|()| format!("Request contains duplicate ID: {}", request.json))?;
                ids.insert(id.clone());

                self.requests.push(request);
            }

            self.requests.reverse();

            Ok(())
        }

        fn run_rpc_calls(&mut self) -> orfail::Result<()> {
            self.base_time = Instant::now();
            self.base_unix_timestamp = std::time::UNIX_EPOCH.elapsed().or_fail()?;

            let ring_entries = self
                .channels
                .len()
                .saturating_mul(2)
                .max(8)
                .next_power_of_two();
            let ring_entries = u32::try_from(ring_entries).or_fail_with(|_| {
                "Too many channels for io-uring queue size".to_owned()
            })?;
            let mut ring = IoUring::new(ring_entries).or_fail()?;

            for channel in &mut self.channels {
                channel.submit_read(&mut ring).or_fail()?;
            }
            ring.submit().or_fail()?;

            while !self.requests.is_empty() || self.ongoing_requests > 0 {
                if self.ongoing_requests < self.concurrency.get() {
                    self.enqueue_pending_requests(&mut ring).or_fail()?;
                }

                ring.submit_and_wait(1).or_fail()?;

                let mut cq = ring.completion();
                for cqe in &mut cq {
                    let (channel_id, op) = decode_user_data(cqe.user_data());
                    let result = cqe.result();
                    let channel = &mut self.channels[channel_id];

                    match op {
                        OP_WRITE => {
                            channel
                                .handle_write_completion(&mut ring, result)
                                .or_fail()?;
                        }
                        OP_READ => {
                            let old_count = channel.ongoing_requests;
                            self.ongoing_requests -= old_count;
                            channel
                                .handle_read_completion(&mut ring, result)
                                .or_fail()?;
                            self.ongoing_requests += channel.ongoing_requests;

                            self.channel_requests.remove(&(old_count, channel_id));
                            self.channel_requests
                                .insert((channel.ongoing_requests, channel_id));
                        }
                        _ => {
                            return Err(orfail::Failure::new(format!(
                                "Unknown io-uring op: {op}"
                            )));
                        }
                    }
                }
            }

            Ok(())
        }

        fn enqueue_pending_requests(&mut self, ring: &mut IoUring) -> orfail::Result<()> {
            let now = Instant::now();
            while self.ongoing_requests < self.concurrency.get()
                && let Some(request) = self.requests.pop()
            {
                let (_, i) = self.channel_requests.pop_first().or_fail()?;
                self.channels[i]
                    .enqueue_request(ring, now, request)
                    .or_fail()?;
                self.channel_requests
                    .insert((self.channels[i].ongoing_requests, i));
                self.ongoing_requests += 1;
            }
            Ok(())
        }

        fn output_results(&self) -> orfail::Result<()> {
            let stdout = std::io::stdout();
            let mut output_writer = std::io::BufWriter::new(stdout.lock());

            for channel in &self.channels {
                let mut requests = channel
                    .requests
                    .iter()
                    .zip(channel.start_times.iter())
                    .map(|(r, t)| (r.id.clone(), (r, *t)))
                    .collect::<std::collections::HashMap<_, _>>();

                for (line, end_time) in std::io::BufReader::new(&channel.recv_buf[..])
                    .lines()
                    .zip(channel.end_times.iter())
                {
                    let line = line.or_fail()?;
                    let mut response = Response::parse(line).or_fail()?;
                    let id = response
                        .id
                        .take()
                        .or_fail_with(|()| "Response missing required 'id' field".to_owned())?;
                    let (request, start_time) = requests.remove(&Some(id)).or_fail_with(|()| {
                        "Response ID does not match any pending request".to_owned()
                    })?;
                    let start_unix_timestamp =
                        start_time.duration_since(self.base_time) + self.base_unix_timestamp;
                    let end_unix_timestamp =
                        end_time.duration_since(self.base_time) + self.base_unix_timestamp;

                    writeln!(
                        output_writer,
                        "{}",
                        nojson::object(|f| {
                            for (name, value) in request.json.value().to_object().expect("bug") {
                                let name = name.to_unquoted_string_str().expect("infallible");
                                f.member(name, value)?;
                            }
                            for (name, value) in response.json.value().to_object().expect("bug") {
                                let name = name.to_unquoted_string_str().expect("infallible");
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
    }

    fn encode_user_data(channel_id: usize, op: u64) -> u64 {
        ((channel_id as u64) << 1) | (op & 1)
    }

    fn decode_user_data(user_data: u64) -> (usize, u64) {
        ((user_data >> 1) as usize, user_data & 1)
    }

    fn io_result_bytes(op: &str, result: i32) -> orfail::Result<usize> {
        if result < 0 {
            let error = std::io::Error::from_raw_os_error(-result);
            return Err(orfail::Failure::new(format!("Failed to {op}: {error}")));
        }
        Ok(result as usize)
    }

    fn push_sqe(ring: &mut IoUring, entry: &Entry) -> orfail::Result<()> {
        unsafe {
            ring.submission()
                .push(entry)
                .map_err(|_| orfail::Failure::new("io-uring submission queue is full"))?;
        }
        Ok(())
    }

    struct RpcChannel {
        id: usize,
        server_addr: ServerAddr,
        stream: std::net::TcpStream,
        send_buf: Vec<u8>,
        send_buf_offset: usize,
        pending_sends: VecDeque<Vec<u8>>,
        recv_buf: Vec<u8>,
        read_buf: [u8; 4096],
        write_inflight: bool,
        read_inflight: bool,
        ongoing_requests: usize,
        requests: Vec<Request>,
        start_times: Vec<Instant>,
        end_times: Vec<Instant>,
    }

    impl RpcChannel {
        fn new(id: usize, server_addr: ServerAddr, stream: std::net::TcpStream) -> Self {
            Self {
                id,
                server_addr,
                stream,
                send_buf: Vec::new(),
                send_buf_offset: 0,
                pending_sends: VecDeque::new(),
                recv_buf: Vec::new(),
                read_buf: [0; 4096],
                write_inflight: false,
                read_inflight: false,
                ongoing_requests: 0,
                requests: Vec::new(),
                start_times: Vec::new(),
                end_times: Vec::new(),
            }
        }

        fn enqueue_request(
            &mut self,
            ring: &mut IoUring,
            now: Instant,
            request: Request,
        ) -> orfail::Result<()> {
            let json_text = request.json.text();
            let mut bytes = Vec::with_capacity(json_text.len() + 1);
            bytes.extend_from_slice(json_text.as_bytes());
            bytes.push(b'\n');

            self.start_times.push(now);
            self.requests.push(request);
            self.ongoing_requests += 1;

            if self.write_inflight {
                self.pending_sends.push_back(bytes);
            } else {
                self.send_buf.extend_from_slice(&bytes);
                self.submit_write(ring).or_fail()?;
            }

            Ok(())
        }

        fn submit_read(&mut self, ring: &mut IoUring) -> orfail::Result<()> {
            if self.read_inflight {
                return Ok(());
            }

            let fd = types::Fd(self.stream.as_raw_fd());
            let entry = opcode::Read::new(fd, self.read_buf.as_mut_ptr(), self.read_buf.len() as _)
                .build()
                .user_data(encode_user_data(self.id, OP_READ));
            push_sqe(ring, &entry).or_fail()?;
            self.read_inflight = true;

            Ok(())
        }

        fn submit_write(&mut self, ring: &mut IoUring) -> orfail::Result<()> {
            if self.write_inflight {
                return Ok(());
            }

            if self.send_buf_offset >= self.send_buf.len() {
                self.send_buf.clear();
                self.send_buf_offset = 0;
                self.fill_send_buf_from_queue();
            }

            if self.send_buf_offset >= self.send_buf.len() {
                return Ok(());
            }

            let fd = types::Fd(self.stream.as_raw_fd());
            let buf = &self.send_buf[self.send_buf_offset..];
            let entry = opcode::Write::new(fd, buf.as_ptr(), buf.len() as _)
                .build()
                .user_data(encode_user_data(self.id, OP_WRITE));
            push_sqe(ring, &entry).or_fail()?;
            self.write_inflight = true;

            Ok(())
        }

        fn handle_write_completion(
            &mut self,
            ring: &mut IoUring,
            result: i32,
        ) -> orfail::Result<()> {
            self.write_inflight = false;

            let n = io_result_bytes("send request", result)?;
            (n > 0).or_fail_with(|()| "Connection closed by server".to_owned())?;

            self.send_buf_offset += n;
            if self.send_buf_offset >= self.send_buf.len() {
                self.send_buf.clear();
                self.send_buf_offset = 0;
                self.fill_send_buf_from_queue();
            }

            self.submit_write(ring).or_fail()?;
            Ok(())
        }

        fn handle_read_completion(
            &mut self,
            ring: &mut IoUring,
            result: i32,
        ) -> orfail::Result<()> {
            self.read_inflight = false;

            let n = io_result_bytes("read response", result)?;
            (n > 0).or_fail_with(|()| "Connection closed by server".to_owned())?;

            let count = self.read_buf[..n].iter().filter(|&&b| b == b'\n').count();
            if count > 0 {
                let now = Instant::now();
                self.end_times.extend(std::iter::repeat_n(now, count));
                self.ongoing_requests = self
                    .ongoing_requests
                    .checked_sub(count)
                    .or_fail_with(|()| "Too many responses".to_owned())?;
            }

            self.recv_buf.extend_from_slice(&self.read_buf[..n]);
            self.submit_read(ring).or_fail()?;

            Ok(())
        }

        fn fill_send_buf_from_queue(&mut self) {
            if !self.send_buf.is_empty() {
                return;
            }

            while let Some(mut next) = self.pending_sends.pop_front() {
                self.send_buf.append(&mut next);
            }
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux::try_run;

#[cfg(not(target_os = "linux"))]
pub fn try_run(args: &mut noargs::RawArgs) -> noargs::Result<bool> {
    use std::num::NonZeroUsize;

    use crate::types::ServerAddr;

    if !noargs::cmd("bench")
        .doc("Run JSON-RPC benchmark")
        .take(args)
        .is_present()
    {
        return Ok(false);
    }

    let _concurrency: NonZeroUsize = noargs::opt("concurrency")
        .short('c')
        .ty("INTEGER")
        .doc("Number of concurrent requests")
        .default("1")
        .take(args)
        .then(|o| o.value().parse())?;

    let server_addr_arg = noargs::arg("<SERVER>...")
        .doc("JSON-RPC server address or hostname")
        .example("127.0.0.1:8080");
    let mut _server_addrs: Vec<ServerAddr> = Vec::new();
    _server_addrs.push(server_addr_arg.take(args).then(|a| a.value().parse())?);
    while let Some(addr) = server_addr_arg
        .take(args)
        .present_and_then(|a| a.value().parse())?
    {
        _server_addrs.push(addr);
    }

    if args.metadata().help_mode {
        return Ok(true);
    }

    eprintln!("bench command requires Linux with io-uring");
    Ok(true)
}
