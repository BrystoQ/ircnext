//! Protocole IRC sans I/O : représentation des messages, analyse, sérialisation
//! et utilitaires associés (tags IRCv3, casemapping, `RPL_ISUPPORT`, formatage mIRC).
//!
//! Cette crate ne fait aucune entrée/sortie : elle transforme des lignes en
//! valeurs typées et inversement, ce qui la rend entièrement testable.

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
