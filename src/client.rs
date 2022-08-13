mod tcp;
mod ws;

use crate::{Packet, LOCAL_ADDRESS};

use std::net::{TcpStream, SocketAddr};
use std::sync::{Mutex, Arc};
use std::time::{Duration, Instant};

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
pub struct Client<P: Packet> {
    incoming: Arc<Mutex<Vec<P>>>,
    outgoing: Arc<Mutex<Vec<P>>>
}

impl <P: Packet + Sync + Send + Clone + 'static>Client<P> {
    /// Attempt to create a new client with `config`
    pub fn launch(config: ClientConfig) -> Option<Client<P>> {
        let outgoing: Arc<Mutex<Vec<P>>> = Arc::new(Mutex::new(Vec::new()));
        let incoming = Arc::new(Mutex::new(Vec::new()));
        #[cfg(not(target_arch = "wasm32"))]
        {
            let target_address = SocketAddr::from((config.address, config.tcp_port));
            let pot_con = TcpStream::connect_timeout(&target_address, config.connection_timeout);
            if let Ok(mut connection) = pot_con {
                let thread_tx = outgoing.clone();
                let thread_rx = incoming.clone();
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
                        let mut rx_access = thread_rx.lock().unwrap();
                        while let Some(packet) = Packet::from_reader(&mut connection) {
                            rx_access.push(packet);
                        }
                        drop(rx_access);
                        let delta_time = last_loop.elapsed();
                        if delta_time < config.tick_delay {
                            std::thread::sleep(config.tick_delay - delta_time);
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
        #[cfg(target_arch = "wasm32")]
        {
            let target_address = format!("wss://{}:{}", config.address, config.ws_port);
            let pot_con = websocket::client::ClientBuilder::new(target_address);
            todo!()
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
        return data;
    }
}
