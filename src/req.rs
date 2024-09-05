use jsonlrpc::{JsonRpcVersion, RequestId, RequestObject, RequestParams};
use orfail::OrFail;

#[derive(Debug, clap::Args)]
pub struct ReqCommand {
    method: String,

    params: Option<RequestParams>,

    #[clap(long)]
    id: Option<RequestId>,
}

impl ReqCommand {
    pub fn run(self) -> orfail::Result<()> {
        let request = RequestObject {
            jsonrpc: JsonRpcVersion::V2,
            method: self.method,
            params: self.params,
            id: self.id,
        };
        serde_json::to_writer(std::io::stdout(), &request).or_fail()?;
        Ok(())
    }
}
