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
//!     /// Since we're not a server, a handler isn't needed and this function can be ignored.
//!     fn handler(&self, _glbs: Arc<Mutex<Globals>>, addr: SocketAddr) -> Vec<(Self, SocketAddr)> {
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
pub mod server;
/// Types and functions for creating a client
pub mod client;

/// Describes a packet format used to communicate data over the network.
pub trait Packet: Sized {
    /// Takes a given reader `R` and attempts to gather a packet from it. Returning None is not
    /// considered an error and should be used to avoid blocking. netty will not work if this
    /// function blocks.
    fn from_reader<R: std::io::Read>(_: &mut R) -> Option<Self>;
    /// Takes a given packet and writes it to a buffer `W`
    fn write<W: std::io::Write + ?Sized>(&self, _: &mut W) -> ();
}

const LOCAL_ADDRESS: [u8; 4] = [127, 0, 0, 1];
const GLOBAL_ADDRESS: [u8; 4] = [0, 0, 0, 0];
