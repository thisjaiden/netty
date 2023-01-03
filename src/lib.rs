//! # netty
//! An opinonated networking engine used for WASM-compatible and consistent design. Not
//! exceptionally fast, secure, or easy, look elsewhere for that.
//! 
//! ### Server example
//! This simple example shows a server that responds to a client sending `Packet::A` with
//! `Packet::B` and vice versa. It assumes the client has the same Packet type, `from_reader` impl,
//! and `write` impl.
//! ```no_run
//! use std::{net::SocketAddr, sync::{Mutex, Arc}};
//! use netty::server::{ServerConfig, launch_server};
//! 
//! enum Packet {
//!     A,
//!     B
//! }
//! 
//! impl netty::Packet<Globals> for Packet {
//!     /// "read" function, probably just a wrapper around some `serde` library
//!     fn from_reader<R: std::io::Read>(reader: &mut R) -> Option<Self> {
//!         unimplemented!()
//!     }
//!     /// "write" function, probably just a wrapper around some `serde` library
//!     fn write<W: std::io::Write + ?Sized>(&self, writer: &mut W) {
//!         unimplemented!()
//!     }
//!     fn handler(&self, _glbs: Arc<Mutex<Globals>>, addr: SocketAddr) -> Vec<(Self, SocketAddr)> {
//!         // Reflect back `A` if we recieved `B` and vice versa
//!         match self {
//!             Self::A => vec![(Self::B, addr)],
//!             Self::B => vec![(Self::A, addr)]
//!         }
//!     }
//! }
//! 
//! #[derive(Clone, Default)]
//! struct Globals {}
//! 
//! // `launch_server` does not return. If you need to keep doing stuff, spawn a thread to run this
//! // function in.
//! launch_server::<Packet, Globals>(ServerConfig::default());
//! ```
//! ### Client example
//! ```no_run
//! use std::{net::SocketAddr, sync::{Mutex, Arc}};
//! use netty::client::{ClientConfig, Client};
//! 
//! #[derive(Clone, Debug)]
//! enum Packet {
//!     A,
//!     B
//! }
//! 
//! impl netty::Packet<Globals> for Packet {
//!     /// "read" function, probably just a wrapper around some `serde` library
//!     fn from_reader<R: std::io::Read>(reader: &mut R) -> Option<Self> {
//!         unimplemented!()
//!     }
//!     /// "write" function, probably just a wrapper around some `serde` library
//!     fn write<W: std::io::Write + ?Sized>(&self, writer: &mut W) {
//!         unimplemented!()
//!     }
//! }
//! 
//! #[derive(Clone, Default)]
//! struct Globals {}
//! 
//! // Launch the client
//! let mut client = Client::launch(ClientConfig::default())
//!     .expect("Unable to connect to the server");
//! 
//! // Send a message to the server
//! client.send(Packet::A);
//! 
//! // Display any recieved packets. In a real environment, there would need to be some time before
//! // a response and there may not be one.
//! for packet in client.get_packets() {
//!     println!("Received a packet {:?}!", packet);
//! }
//! ```

#![warn(missing_docs)]

/// Types and functions for creating a server
#[cfg(not(target_arch = "wasm32"))]
#[cfg(not(feature = "legacy_threaded"))]
pub mod server;
/// Types and functions for creating a client
#[cfg(not(feature = "legacy_threaded"))]
pub mod client;
#[cfg(feature = "legacy_threaded")]
mod legacy;
#[cfg(feature = "legacy_threaded")]
pub use legacy::*;

/// Describes a packet format used to communicate data over the network.
pub trait Packet: Sized {
    /// Takes a given reader `R` and attempts to gather a packet from it. Returning `None` indicates
    /// there is not enough data to build a packet.
    #[cfg(not(feature = "legacy_threaded"))]
    fn from_reader<R: std::io::Read>(_: &mut R) -> Option<Self>;
    #[cfg(feature = "legacy_threaded")]
    fn from_reader<R: std::io::Read>(_: &mut R) -> Self;
    /// Serializes a packet into a byte array. Must be deserializable with `from_reader`.
    fn to_vec(&self) -> Vec<u8>;
    /// Takes a given packet and writes it to a buffer `W`
    fn write<W: std::io::Write + ?Sized>(&self, _: &mut W) -> ();
}

const LOCAL_ADDRESS: [u8; 4] = [127, 0, 0, 1];
const GLOBAL_ADDRESS: [u8; 4] = [0, 0, 0, 0];
