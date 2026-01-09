// # State Store Implementations
//
// This module provides implementations of the StateStore trait for
// different persistence strategies.

pub mod file;
pub mod memory;

pub use file::FileStateStore;
pub use memory::MemoryStateStore;
