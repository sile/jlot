use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JsonRpcVersion {
    #[serde(rename = "2.0")]
    V2,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: JsonRpcVersion,

    pub method: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<RequestParams>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Id>,
}

impl Request {
    pub fn new(method: String, params: Option<RequestParams>, id: Option<Id>) -> Self {
        Self {
            jsonrpc: JsonRpcVersion::V2,
            method,
            params,
            id,
        }
    }
}

impl FromStr for Request {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    Number(i64),
    String(String),
}

impl FromStr for Id {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestParams {
    Array(Vec<serde_json::Value>),
    Object(serde_json::Map<String, serde_json::Value>),
}

impl FromStr for RequestParams {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Response {
    Ok {
        jsonrpc: JsonRpcVersion,
        result: serde_json::Value,
        id: Id,
    },
    Err {
        jsonrpc: JsonRpcVersion,
        error: Error,
        id: Option<Id>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Error {
    pub code: i64,

    pub message: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}
