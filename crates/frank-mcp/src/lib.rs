//! stdio MCP proxy that rewrites tool/prompt descriptions in transit.
//!
//! Not a real MCP implementation — see `proxy.rs`'s module docs. Ported
//! from the historical Caveman MCP proxy.

mod proxy;
mod transform;

pub use proxy::{ProxyConfig, run};
pub use transform::{compress_descriptions_in_place, transform_response};
