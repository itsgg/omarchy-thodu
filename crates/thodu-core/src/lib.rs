//! Thodu's logic with no input or output (SPEC section 4): the target model
//! and coordinates, the accessibility tree to targets, labels, search, the
//! session's keys and the stale check. `thodud` supplies the world.

pub mod config;
pub mod grid;
pub mod input;
pub mod labels;
pub mod model;
pub mod perf;
pub mod protocol;
pub mod session;
pub mod stale;
pub mod tree;
pub mod unregistered;
