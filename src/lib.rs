//! `itok` -- estimate the token / context cost of files and changes, from
//! the command line. `git`- and `du`-shaped, honest by design (the default
//! is a labelled *estimate*, never a claim of truth).
//!
//! This crate is primarily the `itok` **binary**; the library exists so the
//! whole command surface is unit- and property-tested at lib cost (`main.rs`
//! is a thin shell). See the README for usage and `SPEC.md` for the design
//! invariants.

mod args;
#[cfg(feature = "bpe")]
pub mod bpe;
#[cfg(feature = "session")]
mod calibrate;
mod capacity;
mod capcmd;
mod checkcmd;
pub mod cli;
mod diffargs;
mod diffcmd;
#[cfg(feature = "ollama")]
mod discover;
mod docs;
mod doctor;
mod doctorfmt;
mod estcmd;
pub mod estimate;
mod fitcmd;
#[cfg(feature = "session")]
mod gauge;
mod gitref;
mod glob;
mod guardcmd;
#[cfg(feature = "session")]
mod headroom;
mod hook;
pub mod json;
mod logcmd;
mod models;
#[cfg(feature = "ollama")]
mod ollama;
mod policy;
#[cfg(feature = "session")]
mod ratecmd;
pub mod render;
#[cfg(feature = "session")]
pub mod session;
mod showcmd;
#[cfg(test)]
mod testutil;
#[cfg(feature = "session")]
mod topcmd;
#[cfg(feature = "session")]
mod tracecmd;
mod units;
mod verb;
pub mod walk;
