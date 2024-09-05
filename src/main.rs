use clap::Parser;
use orfail::OrFail;

use jlot::{batch_call::BatchCallCommand, call::CallCommand};

#[derive(Parser)]
#[clap(version)]
enum Args {
    Call(CallCommand),
    BatchCall(BatchCallCommand),
    // TODO: bench, echo-server
}

fn main() -> orfail::Result<()> {
    let args = Args::parse();
    match args {
        Args::Call(c) => c.run().or_fail()?,
        Args::BatchCall(c) => c.run().or_fail()?,
    }
    Ok(())
}
