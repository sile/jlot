use clap::Parser;
use jrot::command_call::CallCommand;
use orfail::OrFail;

#[derive(Parser)]
#[clap(version)]
enum Args {
    Call(CallCommand),
}

fn main() -> orfail::Result<()> {
    let args = Args::parse();
    match args {
        Args::Call(c) => c.run().or_fail()?,
    }
    Ok(())
}
