//! Versioned execution controls shared by the textbook review agent and its evaluations.
pub mod completion;
pub mod context;
pub mod execution;
pub mod policy;
pub mod process;
pub mod tools;

pub const VERSION: &str = "review-harness-v2";

#[cfg(test)]
mod cancellation_tests;
#[cfg(test)]
mod completion_tests;
#[cfg(test)]
mod context_tests;
#[cfg(test)]
mod fault_tests;
#[cfg(test)]
mod policy_tests;
#[cfg(test)]
mod process_tests;
#[cfg(test)]
mod tool_tests;
