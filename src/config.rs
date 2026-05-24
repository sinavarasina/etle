//! Configuration loading and minimal TOML-like parsing.

mod parse;
mod prelude;

pub mod constants;
pub mod error;
pub mod load;
pub mod model;

#[cfg(test)]
mod tests;
