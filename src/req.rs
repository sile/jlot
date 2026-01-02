use std::num::NonZeroUsize;

use jsonlrpc::{JsonRpcVersion, RequestId, RequestObject, RequestParams};
use orfail::OrFail;

/// Generate a JSON-RPC request object JSON.
pub struct ReqCommand {
    /// Method name.
    pub method: String,

    /// Request parameters (JSON array or JSON object).
    pub params: Option<RequestParams>,

    /// Request ID (number or string).
    pub id: RequestId,

    /// When set, the "id" field will be excluded from the resulting JSON object.
    pub notification: bool,

    /// Count of requests to generate.
    pub count: NonZeroUsize,
}

impl ReqCommand {
    pub fn parse() -> noargs::Result<Self> {
        let mut args = noargs::raw_args();

        args.metadata_mut().app_name = env!("CARGO_PKG_NAME");
        args.metadata_mut().app_description = env!("CARGO_PKG_DESCRIPTION");

        // Handle help flag
        noargs::HELP_FLAG.take_help(&mut args);

        // Parse positional arguments and options
        let method: String = noargs::arg("<METHOD>")
            .doc("Method name")
            .take(&mut args)
            .then(|a| a.value().parse())?;

        let params: Option<RequestParams> = noargs::arg("[PARAMS]")
            .doc("Request parameters (JSON array or JSON object)")
            .take(&mut args)
            .present_and_then(|a| {
                let json_str = a.value();
                serde_json::from_str(json_str).map_err(|e| format!("invalid JSON: {}", e))
            })?;

        let id: RequestId = noargs::opt("id")
            .short('i')
            .doc("Request ID (number or string)")
            .default("0")
            .take(&mut args)
            .then(|o| -> Result<RequestId, std::convert::Infallible> {
                let val = o.value();
                // Try parsing as number first, otherwise use as string
                val.parse::<i64>()
                    .map(RequestId::Number)
                    .or_else(|_| Ok(RequestId::String(val.to_string())))
            })?;

        let notification: bool = noargs::flag("notification")
            .short('n')
            .doc("Exclude the \"id\" field from the resulting JSON object")
            .take(&mut args)
            .is_present();

        let count: NonZeroUsize = noargs::opt("count")
            .short('c')
            .doc("Count of requests to generate")
            .default("1")
            .take(&mut args)
            .then(|o| {
                o.value()
                    .parse()
                    .map_err(|_| "count must be a positive number".to_string())
            })?;

        // Finish parsing and handle help
        if let Some(help) = args.finish()? {
            print!("{}", help);
            std::process::exit(0);
        }

        Ok(ReqCommand {
            method,
            params,
            id,
            notification,
            count,
        })
    }

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
