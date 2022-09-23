mod tcp;
mod ws;

use websocket::OwnedMessage;
use websocket::sync::Client;

use crate::Packet;

use std::any::Any;
use std::io::Cursor;
use std::net::{TcpStream, SocketAddr};
use std::thread;
use std::time::{Duration, Instant};
use std::sync::mpsc::{Sender, Receiver};
use std::sync::{mpsc, Mutex, Arc};

/// Describes the configuration that a server will use
#[derive(Clone, Copy)]
pub struct ServerConfig<P, G> {
    /// Number of threads to create for distributing tasks to
    pub worker_threads: usize,
    /// Switch for if this server should host this locally or attempt to bind to the wider net
    pub public_facing: bool,
    /// Port to accept TCP connections on
    pub tcp_port: u16,
    /// Port to accept websocket connections on
    pub ws_port: u16,
    /// Minimum delay between logic ticks
    pub tick_delay: Duration,
    /// `handler` runs once for every packet the server recieves with access to a global state and
    /// the address of where the packet was recieved from. It returns a list of packets to be sent
    /// to the addresses paired with them.
    pub handler: fn(P, Arc<Mutex<G>>, SocketAddr) -> Vec<(P, SocketAddr)>,
    /// `tick` runs once every [tick_delay] with access to the global state. It returns a list of
    /// packets to be sent to the addresses paired with them.
    pub tick: fn(Arc<Mutex<G>>) -> Vec<(P, SocketAddr)>,
}

impl <P, G>Default for ServerConfig<P, G> {
    fn default() -> Self {
        Self {
            worker_threads: 3,
            public_facing: false,
            tcp_port: 8000,
            ws_port: 8001,
            tick_delay: Duration::from_secs_f32(1.0 / 60.0),
            handler: |_, _, _| { vec![] },
            tick: |_| { vec![] }
        }
    }
}

/// Attempt to create a new server with `config` and launch it on the network
pub fn launch_server<P: Packet + Sync + Send + 'static + Clone, G: Any + Clone + Default + Sync + Send + Clone>(config: ServerConfig<P, G>) -> ! {
    let state = Arc::new(Mutex::new(G::default()));
    let tcp = tcp::listener(config.clone());
    // let mut ws = ws::listener(config.clone());
    let rxbuf = Arc::new(Mutex::new(Vec::new()));
    let txbuf: Arc<Mutex<Vec<(P, SocketAddr)>>> = Arc::new(Mutex::new(Vec::new()));
    
    loop {
        let mut a = tcp.accept().unwrap();
        let mut a2 = (a.0.try_clone().unwrap(), a.1.clone());
        let thread_rx = rxbuf.clone();
        let thread_tx = txbuf.clone();
        thread::spawn(move || {
            loop {
                let packet = P::from_reader(&mut a.0);
                let mut rx_access = thread_rx.lock().unwrap();
                rx_access.push(packet);
                drop(rx_access);
            }
        });
        thread::spawn(move || {
            loop {
                let mut tx_access = thread_tx.lock().unwrap();
                let mut wrote_index = None;
                for (index, (data, loc)) in tx_access.iter().enumerate() {
                    if loc == &a2.1 {
                        data.write(&mut a2.0);
                        wrote_index = Some(index);
                        break;
                    }
                }
                if let Some(index) = wrote_index {
                    tx_access.remove(index);
                }
                drop(tx_access);
                std::thread::sleep(config.tick_delay);
            }
        });
    }
}
