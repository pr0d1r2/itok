//! itok core. The logic lives in a lib so it is unit-tested in the fast
//! gate (kind(lib)); `main.rs` is a thin bin over it. Spec: SPEC.md.

mod args;
#[cfg(feature = "bpe")]
pub mod bpe;
mod checkcmd;
pub mod cli;
mod diffargs;
mod diffcmd;
#[cfg(feature = "ollama")]
mod discover;
mod docs;
mod doctor;
mod estcmd;
pub mod estimate;
mod fitcmd;
mod gitref;
pub mod json;
mod logcmd;
mod models;
#[cfg(feature = "ollama")]
mod ollama;
pub mod render;
mod showcmd;
#[cfg(test)]
mod testutil;
mod units;
mod verb;
pub mod walk;
