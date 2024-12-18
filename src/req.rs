use std::num::NonZeroUsize;

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
    #[clap(long, short, default_value = "0")]
    id: RequestId,

    /// When set, the "id" field will be excluded from the resulting JSON object.
    #[clap(short, long)]
    notification: bool,

    /// Count of requests to generate.
    #[clap(short, long, default_value = "1")]
    count: NonZeroUsize,
}

impl ReqCommand {
    pub fn run(self) -> orfail::Result<()> {
        let request = RequestObject {
            jsonrpc: JsonRpcVersion::V2,
            method: self.method,
            params: self.params,
            id: (!self.notification).then_some(self.id),
        };

        let json = serde_json::to_string(&request).or_fail()?;
        for _ in 0..self.count.get() {
            println!("{json}");
        }
        Ok(())
    }
}
