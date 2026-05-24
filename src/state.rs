//! Local ETLE library state, progress, and chunk persistence.

mod prelude;

pub mod codec;
pub mod library;
pub mod model;
pub mod paths;
pub mod storage;

#[cfg(test)]
mod tests;
