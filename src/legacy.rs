pub mod client {
    use crate::{Packet, LOCAL_ADDRESS};

    use std::io::Cursor;
    use std::marker::PhantomData;
    use std::pin::Pin;
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
                    loop {
                        let pkt = P::from_reader(&mut rx_clone);
                        let mut rx_access = thread_rx.lock().unwrap();
                        rx_access.push(pkt);
                        drop(rx_access);
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
            return data;
        }
    }
}

pub mod server {
    use crate::{Packet, LOCAL_ADDRESS, GLOBAL_ADDRESS};

    use std::any::Any;
    use std::io::Cursor;
    use std::net::{TcpStream, SocketAddr};
    use std::thread;
    use std::time::{Duration, Instant};
    use std::sync::{Mutex, Arc};

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
        let address = if config.public_facing {
            GLOBAL_ADDRESS
        }
        else {
            LOCAL_ADDRESS
        };
        let listener = std::net::TcpListener::bind(SocketAddr::from((address, config.tcp_port))).expect("Unable to bind.");
        let outgoing = Arc::new(Mutex::new(Vec::new()));
        
        let tick_copy = state.clone();
        let tick_out_copy = outgoing.clone();
        std::thread::spawn(move || {
            let mut last_loop = Instant::now();
            let mut delta_time;
            loop {
                let mut result = (config.tick)(tick_copy.clone());
                let mut out_access = tick_out_copy.lock().unwrap();
                out_access.append(&mut result);
                drop(out_access);
                delta_time = last_loop.elapsed();
                if delta_time < config.tick_delay {
                    std::thread::sleep(config.tick_delay - delta_time);
                }
                last_loop = Instant::now();
            }
        });

        while let Ok((mut stream, source_addr)) = listener.accept() {
            let client_cp1 = state.clone();
            let client_cp2 = outgoing.clone();
            let client_cp3 = outgoing.clone();
            let mut client_cp4 = stream.try_clone().unwrap();
            std::thread::spawn(move || {
                loop {
                    let pkt = P::from_reader(&mut stream);
                    let mut result = (config.handler)(pkt, client_cp1.clone(), source_addr);
                    let mut out_access = client_cp2.lock().unwrap();
                    out_access.append(&mut result);
                    drop(out_access);
                }
            });
            std::thread::spawn(move || {
                let mut last_loop = Instant::now();
                let mut delta_time;
                loop {
                    let mut out_access = client_cp3.lock().unwrap();
                    loop {
                        let mut mutual = None;
                        for (index, (pkt, addr)) in out_access.iter().enumerate() {
                            if &source_addr == addr {
                                pkt.write(&mut client_cp4);
                                mutual = Some(index);
                                break;
                            }
                        }
                        if let Some(val) = mutual {
                            out_access.swap_remove(val);
                        }
                        else {
                            break;
                        }
                    }
                    drop(out_access);
                    delta_time = last_loop.elapsed();
                    if delta_time < config.tick_delay {
                        std::thread::sleep(config.tick_delay - delta_time);
                    }
                    last_loop = Instant::now();
                }
            });
        }
        unreachable!();
    }
}