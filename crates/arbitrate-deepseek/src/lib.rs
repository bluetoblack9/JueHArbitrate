//! DeepSeek provider request infrastructure.

pub mod transport;

pub mod han;

pub mod wire;

pub use arbitrate_core::types::format_hit_ratio;

mod client;
mod error;
mod retry;

pub use client::{Client, DEFAULT_BASE_URL};
pub use error::{ApiError, Error};
