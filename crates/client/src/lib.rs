//! IRC client: TCP/TLS connections, IRCv3 registration (CAP, SASL, STS) and
//! tracking of the network state.
//!
//! The protocol logic ([`registration`], [`network`]) has no I/O and is
//! tested on its own; only [`client`] and [`connection`] touch the network.

pub mod client;
pub mod config;
pub mod connection;
pub mod network;
pub mod registration;
mod throttle;

pub use client::{Closed, Event, Handle, connect};
pub use config::{Config, Sasl, Secret};
pub use network::{Batch, Channel, Member, Network, whox_request};
pub use registration::Registered;

pub use ircnext_proto as proto;
