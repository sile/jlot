use std::{fmt::Display, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename = "2.0")]
pub struct JsonRpcVersion2;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: JsonRpcVersion2,

    pub method: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<RequestParams>,

    pub id: Id,
}

impl Request {
    pub fn new(method: String, params: Option<RequestParams>, id: Id) -> Self {
        Self {
            jsonrpc: JsonRpcVersion2,
            method,
            params,
            id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    Number(u64),
    String(String),
}

impl Display for Id {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        serde_json::to_value(self)
            .map_err(|_| std::fmt::Error)?
            .fmt(f)
    }
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

impl Display for RequestParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        serde_json::to_value(self)
            .map_err(|_| std::fmt::Error)?
            .fmt(f)
    }
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
        jsonrpc: JsonRpcVersion2,
        result: serde_json::Value,
        id: Id,
    },
    Err {
        jsonrpc: JsonRpcVersion2,
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
