use crate::{Packet, LOCAL_ADDRESS};

use std::io::Cursor;
use std::marker::PhantomData;
use std::pin::Pin;
use std::net::{TcpStream, SocketAddr};
use std::sync::{Mutex, Arc};
use std::time::{Duration, Instant};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(module = "/src/netty.js")]
extern "C" {
    fn start_ws(address: &str) -> u32;
    fn get_ws() -> Box<[u8]>;
    fn send_ws(input_data: &[u8]);
}

/// Describes the configuration that a client will use
pub struct ClientConfig {
    /// What address to connect to
    pub address: [u8; 4],
    /// What port to use for TCP connections
    pub tcp_port: u16,
    /// What port to use for WS connections
    pub ws_port: u16,
    /// Time to wait for a connection to be created
    pub connection_timeout: Duration,
    /// Minimum delay between reads from the network
    pub tick_delay: Duration
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            address: LOCAL_ADDRESS,
            tcp_port: 8000,
            ws_port: 8001,
            connection_timeout: Duration::from_secs(3),
            tick_delay: Duration::from_secs_f32(1.0 / 60.0)
        }
    }
}

/// Represents a client and its connection to a server. Used to read and write from the network
#[cfg(not(target_arch = "wasm32"))]
pub struct Client<P: Packet> {
    incoming: Arc<Mutex<Vec<P>>>,
    outgoing: Arc<Mutex<Vec<P>>>
}

#[cfg(target_arch = "wasm32")]
pub struct Client<P> {
    _p: PhantomData<P>
}

#[cfg(not(target_arch = "wasm32"))]
impl <P: Packet + Sync + Send + Clone + 'static>Client<P> {
    /// Attempt to create a new client with `config`
    pub fn launch(config: ClientConfig) -> Option<Client<P>> {
        let outgoing: Arc<Mutex<Vec<P>>> = Arc::new(Mutex::new(Vec::new()));
        let incoming = Arc::new(Mutex::new(Vec::new()));
        let target_address = SocketAddr::from((config.address, config.tcp_port));
        let pot_con = TcpStream::connect_timeout(&target_address, config.connection_timeout);
        if let Ok(mut connection) = pot_con {
            let thread_tx = outgoing.clone();
            let thread_rx = incoming.clone();
            let mut rx_clone = connection.try_clone().unwrap();
            std::thread::spawn(move || {
                let mut last_loop = Instant::now();
                loop {
                    let mut tx_access = thread_tx.lock().unwrap();
                    let to_tx = tx_access.clone();
                    tx_access.clear();
                    drop(tx_access);
                    for packet in to_tx {
                        packet.write(&mut connection);
                    }
                    
                    let delta_time = last_loop.elapsed();
                    if delta_time < config.tick_delay {
                        std::thread::sleep(config.tick_delay - delta_time);
                    }
                    else {
                        println!("loop took {:?} which is greater than {:?}, not sleeping!", delta_time, config.tick_delay);
                    }
                    last_loop = Instant::now();
                }
            });
            std::thread::spawn(move || {
                let mut last_loop = Instant::now();
                loop {
                    let pkt = P::from_reader(&mut rx_clone).expect("Unable to deserialise TCP!");
                    let mut rx_access = thread_rx.lock().unwrap();
                    rx_access.push(pkt);
                    drop(rx_access);
                    let delta_time = last_loop.elapsed();
                    if delta_time < config.tick_delay {
                        std::thread::sleep(config.tick_delay - delta_time);
                    }
                    else {
                        println!("loop took {:?} which is greater than {:?}, not sleeping!", delta_time, config.tick_delay);
                    }
                    last_loop = Instant::now();
                }
            });
            Some(Client {
                incoming, outgoing
            })
        }
        else {
            None
        }
    }
    
    /// Sends a packet to the remote server
    pub fn send(&mut self, packet: P) {
        let mut tx_access = self.outgoing.lock().unwrap();
        tx_access.push(packet);
        drop(tx_access);
    }
    /// Sends a vector of packets to the remote server
    pub fn send_vec(&mut self, mut packets: Vec<P>) {
        let mut tx_access = self.outgoing.lock().unwrap();
        tx_access.append(&mut packets);
        drop(tx_access);
    }
    /// Grabs all buffered packets from the remote server. Does not always contain elements, does
    /// not block
    pub fn get_packets(&mut self) -> Vec<P> {
        let mut rx_access = self.incoming.lock().unwrap();
        let data = rx_access.clone();
        rx_access.clear();
        if !data.is_empty() {
            println!("Client got a packet");
        }
        return data;
    }
}

#[cfg(target_arch = "wasm32")]
static mut conner: Option<Pin<Box<wasm_sockets::PollingClient>>> = None;

#[cfg(target_arch = "wasm32")]
impl <P: Packet + Sync + Send + Clone + 'static>Client<P> {
    pub fn launch(config: ClientConfig) -> Option<Client<P>> {
        use wasm_bindgen::prelude::*;
        use std::borrow::{Borrow, BorrowMut};
        let target_address = format!(
            "ws://{}.{}.{}.{}:{}",
            config.address[0],
            config.address[1],
            config.address[2],
            config.address[3],
            config.ws_port
        );
        if start_ws(&target_address) == 0 {
            Some(Client {
                _p: PhantomData
            })
        }
        else {
            None
        }
    }
    /// Sends a packet to the remote server
    pub fn send(&mut self, packet: P) {
        let mut vect = vec![];
        packet.write(&mut vect);
        send_ws(&vect);
    }
    /// Sends a vector of packets to the remote server
    pub fn send_vec(&mut self, mut packets: Vec<P>) {
        let mut buf = vec![];
        for packet in packets {
            buf.clear();
            packet.write(&mut buf);
            send_ws(&buf);
        }
    }
    /// Grabs all buffered packets from the remote server. Does not always contain elements, does
    /// not block
    pub fn get_packets(&mut self) -> Vec<P> {
        // TODO GET MULTIPLE PACKETS AT ONCE
        let mut data = get_ws();
        let mut packets = vec![];
        if data.len() > 0 {
            if let Some(packet) = P::from_reader(&mut Cursor::new(data)) {
                packets.push(packet);
            }
            else {
                println!("NOT ENOUGH DATAA!!!!");
            }
        }
        return packets;
    }
}
