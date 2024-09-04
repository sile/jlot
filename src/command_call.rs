#[derive(Debug, clap::Args)]
pub struct CallCommand {}

impl CallCommand {
    pub fn run(&self) -> orfail::Result<()> {
        Ok(())
    }
}
