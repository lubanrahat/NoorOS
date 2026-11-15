//! Noor Shell: parsing and command execution.

pub mod commands;
pub mod parser;

pub use commands::{Action, execute};
