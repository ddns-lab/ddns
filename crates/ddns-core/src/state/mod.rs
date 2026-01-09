// # State Store Implementations
//
// This module provides implementations of the StateStore trait for
// different persistence strategies.

pub mod memory;
pub mod file;

pub use memory::MemoryStateStore;
pub use file::FileStateStore;
