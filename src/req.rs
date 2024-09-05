use jsonlrpc::{JsonRpcVersion, RequestId, RequestObject, RequestParams};
use orfail::OrFail;

/// Generate a JSON-RPC request object JSON.
#[derive(Debug, clap::Args)]
pub struct ReqCommand {
    /// Method name.
    method: String,

    /// Request parameters (JSON array or JSON object).
    params: Option<RequestParams>,

    /// Request ID (number or string).
    ///
    /// If not provided, the request is regarded as a notification.
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
