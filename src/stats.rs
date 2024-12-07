use std::{io::Write, time::Duration};

use jsonlrpc::JsonlStream;
use orfail::OrFail;
use serde::Serialize;

use crate::{
    call::{Metadata, Output},
    io,
};

/// Calculate statistics from JSON objects outputted by executing the command `call --add-metadata ...`.
///
/// Note that the output of `call` command does not include notifications,
/// so the statistics do not take them into account.
#[derive(Debug, clap::Args)]
pub struct StatsCommand {
    /// When set, the `count` field is included in the resulting JSON object.
    #[clap(long)]
    pub count: bool,

    /// When set, the `bps` field is included in the resulting JSON object.
    #[clap(long)]
    pub bps: bool,
}

impl StatsCommand {
    pub fn run(self) -> orfail::Result<()> {
        let stdin = std::io::stdin();
        let mut stream = JsonlStream::new(stdin.lock());
        let mut stats = Stats::default();
        if self.count {
            stats.count = Some(Counter::default());
        }
        if self.bps {
            stats.bps = Some(Bps::default());
        }
        while let Some(output) = io::maybe_eos(stream.read_value::<Output>()).or_fail()? {
            stats.handle_output(output);
        }
        stats.finalize();
        println!("{}", serde_json::to_string(&stats).or_fail()?);
        Ok(())
    }
}

#[derive(Debug, Default, Serialize)]
struct Stats {
    rpc_calls: usize,
    duration: f64,
    max_concurrency: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<Counter>,
    rps: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    bps: Option<Bps>,
    latency: Latency,

    #[serde(skip)]
    start_end_times: Vec<(Duration, Duration)>,

    #[serde(skip)]
    latencies: Vec<Duration>,

    #[serde(skip)]
    outgoing_bytes: u64,

    #[serde(skip)]
    incoming_bytes: u64,
}

impl Stats {
    fn finalize(&mut self) {
        self.duration = self
            .start_end_times
            .iter()
            .map(|(_, end)| *end)
            .max()
            .unwrap_or_default()
            .saturating_sub(
                self.start_end_times
                    .iter()
                    .map(|(start, _)| *start)
                    .min()
                    .unwrap_or_default(),
            )
            .as_secs_f64();

        if self.duration > 0.0 {
            if let Some(bps) = &mut self.bps {
                bps.incoming = (self.incoming_bytes * 8) as f64 / self.duration;
                bps.outgoing = (self.outgoing_bytes * 8) as f64 / self.duration;
            }

            self.rps = self.rpc_calls as f64 / self.duration;
        }

        if !self.latencies.is_empty() {
            self.latencies.sort();
            self.latency.min = self.latencies.first().expect("unreachable").as_secs_f64();
            self.latency.p25 = self.latencies[self.latencies.len() / 4].as_secs_f64();
            self.latency.p50 = self.latencies[self.latencies.len() / 2].as_secs_f64();
            self.latency.p75 = self.latencies[self.latencies.len() * 3 / 4].as_secs_f64();
            self.latency.max = self.latencies.last().expect("unreachable").as_secs_f64();
            self.latency.avg = (self.latencies.iter().sum::<Duration>()
                / self.latencies.len() as u32)
                .as_secs_f64();
        }

        self.start_end_times.sort();
        for i in 0..self.start_end_times.len() {
            let (start, _end) = self.start_end_times[i];
            let concurrency = self.start_end_times[..i]
                .iter()
                .rev()
                .take_while(|x| start < x.1)
                .count()
                + 1;
            self.max_concurrency = self.max_concurrency.max(concurrency);
        }
    }

    fn handle_output(&mut self, output: Output) {
        self.rpc_calls += 1;

        if let Some(counter) = &mut self.count {
            if output.is_batch() {
                counter.batch_calls += 1;
            }

            counter.requests += output.len();
            for res in output.iter() {
                if res.response.to_std_result().is_ok() {
                    counter.responses.ok += 1;
                } else {
                    counter.responses.error += 1;
                }
            }

            if output.iter().all(|res| res.metadata.is_none()) {
                counter.missing_metadata_calls += 1;
            }
        }

        if let Some(metadata) = output.iter().find_map(|res| res.metadata.as_ref()) {
            self.handle_metadata(metadata, &output);
        }
    }

    fn handle_metadata(&mut self, metadata: &Metadata, output: &Output) {
        self.start_end_times
            .push((metadata.start_time, metadata.end_time));
        self.latencies
            .push(metadata.end_time.saturating_sub(metadata.start_time));

        let mut bytes = Bytes::default();
        for res in output.iter().map(|x| &x.response) {
            serde_json::to_writer(&mut bytes, res).expect("unreachable");
        }
        self.incoming_bytes += bytes.0 as u64;

        let mut bytes = Bytes::default();
        serde_json::to_writer(&mut bytes, &metadata.request).expect("unreachable");
        self.outgoing_bytes += bytes.0 as u64;
    }
}

#[derive(Debug, Default, Serialize)]
struct Counter {
    batch_calls: usize,
    missing_metadata_calls: usize,

    requests: usize,
    responses: OkOrError,
}

#[derive(Debug, Default, Serialize)]
struct OkOrError {
    ok: usize,
    error: usize,
}

#[derive(Debug, Default, Serialize)]
struct Latency {
    min: f64,
    p25: f64,
    p50: f64,
    p75: f64,
    max: f64,
    avg: f64,
}

#[derive(Debug, Default, Serialize)]
struct Bps {
    outgoing: f64,
    incoming: f64,
}

#[derive(Debug, Default)]
struct Bytes(usize);

impl Write for Bytes {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
