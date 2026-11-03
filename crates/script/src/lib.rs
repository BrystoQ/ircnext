//! Interpreter for mIRC scripts (mSL), without I/O: everything that touches
//! the client goes through [`Host`].

pub mod builtins;
pub mod condition;
pub mod events;
pub mod interp;
pub mod parse;

pub use events::Event;
pub use interp::{Error, Frame, Host, Interpreter};
pub use parse::{ParseError, Script, Stmt, parse};

#[cfg(test)]
mod tests;
