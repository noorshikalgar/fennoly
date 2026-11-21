//! Domain rules shared by every Fennoly component.
//!
//! This crate has no I/O. It holds the rules that the server, the future
//! desktop client and the tests must agree on: what a valid file name is,
//! how name conflicts are resolved, and how permissions compare.

pub mod names;
pub mod perm;

pub use names::{NameError, unique_name, validate_name};
pub use perm::{Permission, Role};
