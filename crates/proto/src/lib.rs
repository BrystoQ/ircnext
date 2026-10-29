//! IRC protocol without I/O: message representation, parsing, serialization
//! and related utilities (IRCv3 tags, casemapping, `RPL_ISUPPORT`, mIRC
//! formatting).
//!
//! This crate does no input/output: it turns lines into typed values and
//! back, which makes it fully testable.

pub mod casemap;
pub mod format;
pub mod ircv3;
pub mod isupport;
pub mod message;
pub mod numeric;
pub mod tags;
pub mod text;

pub use casemap::CaseMapping;
pub use isupport::ISupport;
pub use message::{Command, Message, ParseError, Source};
pub use tags::Tags;
