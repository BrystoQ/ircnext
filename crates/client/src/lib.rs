//! Client IRC : connexions TCP/TLS, enregistrement IRCv3 (CAP, SASL, STS)
//! et reconstruction de l'état du réseau.
//!
//! La logique protocolaire ([`registration`], [`network`]) est sans I/O et
//! testée isolément ; seul [`client`] et [`connection`] touchent au réseau.

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
