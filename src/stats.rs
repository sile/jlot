use jsonlrpc::JsonlStream;
use orfail::OrFail;
use serde::Serialize;

use crate::{io, stream_call::Output};

/// Calculate statistics from JSON objects outputted by executing the command `stream-call --add-metadata ...`.
///
/// Note that the output of `stream-call` command does not include notifications,
/// so the statistics do not take them into account.
#[derive(Debug, clap::Args)]
pub struct StatsCommand {}

impl StatsCommand {
    pub fn run(self) -> orfail::Result<()> {
        let stdin = std::io::stdin();
        let mut stream = JsonlStream::new(stdin.lock());
        let mut stats = Stats::default();
        while let Some(output) = io::maybe_eos(stream.read_object::<Output>()).or_fail()? {
            stats.handle_output(output);
        }
        println!("{}", serde_json::to_string(&stats).or_fail()?);
        Ok(())
    }
}

#[derive(Debug, Default, Serialize)]
struct Stats {
    output_count: usize,
    non_metadata_output_count: usize,
    batch_output_count: usize,

    ok_response_count: usize,
    error_response_count: usize,

    min_latency: f64,
    avg_latency: f64,
    max_latency: f64,

    incoming_bps: f64,
    outgoing_bps: f64,

    incoming_bytes: usize,
    outgoing_bytes: usize,
}

impl Stats {
    fn handle_output(&mut self, output: Output) {
        self.output_count += 1;

        // if matches!(output.response, ResponseObject::Ok { .. }) {
        //     self.ok_count += 1;
        // } else {
        //     self.error_count += 1;
        // }

        let Some(metadata) = output.metadata else {
            self.non_metadata_output_count += 1;
            return;
        };
    }
}
