use clap::Parser;
use orfail::OrFail;

use jrot::{command_batch_call::BatchCallCommand, command_call::CallCommand};

#[derive(Parser)]
#[clap(version)]
enum Args {
    Call(CallCommand),
    BatchCall(BatchCallCommand),
}

fn main() -> orfail::Result<()> {
    let args = Args::parse();
    match args {
        Args::Call(c) => c.run().or_fail()?,
        Args::BatchCall(c) => c.run().or_fail()?,
    }
    Ok(())
}
