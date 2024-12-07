use std::{convert::Infallible, str::FromStr};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ServerAddr(pub String);

impl FromStr for ServerAddr {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with(':') {
            Ok(Self(format!("127.0.0.1{s}")))
        } else {
            Ok(Self(s.to_owned()))
        }
    }
}
