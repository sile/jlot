use jsonlrpc::JsonlStream;
use orfail::OrFail;
use serde::Serialize;

use crate::{io, stream_call::Output};

/// Calculate statistics from JSON objects outputted by executing the command `stream-call --add-metadata ...`.
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
struct Stats {}

impl Stats {
    fn handle_output(&mut self, output: Output) {
        println!("{:?}", output);
    }
}
