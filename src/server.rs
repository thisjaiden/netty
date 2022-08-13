mod tcp;
mod ws;

use websocket::sync::Client;

use crate::Packet;

use std::any::Any;
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
    let mut ws = ws::listener(config.clone());
    let (ptx, prx): (Sender<Task<P>>, Receiver<Task<P>>) = mpsc::channel();
    
    let mut task_dispatcher = TaskDispatcher::new(prx);
    for _ in 0..config.worker_threads {
        let thread_config = config.clone();
        let thread_state = state.clone();
        let (tx, rx): (Sender<Task<P>>, Receiver<Task<P>>) = mpsc::channel();
        task_dispatcher.add_channel(tx);
        let ptxc = ptx.clone();
        thread::spawn(move || {
            worker_thread::<P, G>(rx, ptxc, thread_state, thread_config);
        });
    }
    let mut last_loop = Instant::now();
    loop {
        task_dispatcher.dispatch(Task::Tick);
        let potential_connection = tcp.accept();
        if let Ok(connection) = potential_connection {
            task_dispatcher.dispatch(Task::TcpConnection(connection));
        }
        let potential_connection = ws.accept();
        if let Ok(handshake) = potential_connection {
            if let Ok(connection) = handshake.accept() {
                let remote = connection.peer_addr().unwrap();
                task_dispatcher.dispatch(Task::WsConnection((connection, remote)));
            }
        }
        let delta_time = last_loop.elapsed();
        if delta_time < config.tick_delay {
            std::thread::sleep(config.tick_delay - delta_time);
        }
        task_dispatcher.process_returns();
        last_loop = Instant::now();
    }
}

fn worker_thread<P: Packet + Sync + Send, G: Any + Clone + Default + Sync + Send>(task_dispatcher: Receiver<Task<P>>, return_task: Sender<Task<P>>, globals: Arc<Mutex<G>>, config: ServerConfig<P, G>) {
    let mut tcp_connections = vec![];
    let mut ws_connections = vec![];
    loop {
        if let Ok(task) = task_dispatcher.try_recv() {
            match task {
                Task::TcpConnection(con) => {
                    tcp_connections.push(con);
                },
                Task::WsConnection(con) => {
                    ws_connections.push(con);
                },
                Task::Message(msg, loc) => {
                    let mut sent = false;
                    for (con, addr) in tcp_connections.iter_mut() {
                        if addr == &loc {
                            msg.write(con);
                            sent = true;
                            break;
                        }
                    }
                    if !sent {
                        for (con, addr) in ws_connections.iter_mut() {
                            if addr == &loc {
                                let writer = con.writer_mut();
                                msg.write(writer);
                                sent = true;
                                break;
                            }
                        }
                    }
                    if !sent {
                        return_task.send(Task::Message(msg, loc)).unwrap();
                    }
                },
                Task::Tick => {
                    let outgoing = (config.tick)(globals.clone());
                    for (msg, loc) in outgoing {
                        let mut sent = false;
                        for (con, addr) in tcp_connections.iter_mut() {
                            if addr == &loc {
                                msg.write(con);
                                sent = true;
                                break;
                            }
                        }
                        if !sent {
                            for (con, addr) in ws_connections.iter_mut() {
                                if addr == &loc {
                                    let writer = con.writer_mut();
                                    msg.write(writer);
                                    sent = true;
                                    break;
                                }
                            }
                        }
                        if !sent {
                            return_task.send(Task::Message(msg, loc)).unwrap();
                        }
                    }
                }
            }
        }
        let mut outgoing_packs = vec![];
        for con in tcp_connections.iter_mut() {
            let read_attempt: Option<P> = P::from_reader(&mut con.0);
            if let Some(packet) = read_attempt {
                let sendables = (config.handler)(packet, globals.clone(), con.1);
                for pack in sendables {
                    outgoing_packs.push(pack);
                }
            }
        }
        for (packet, loc) in outgoing_packs {
            let mut sent = false;
            for (con, addr) in tcp_connections.iter_mut() {
                if loc == *addr {
                    packet.write(con);
                    sent = true;
                    break;
                }
            }
            if !sent {
                return_task.send(Task::Message(packet, loc)).unwrap();
            }
        }
    }
}

enum Task<P: Packet> {
    TcpConnection((TcpStream, SocketAddr)),
    WsConnection((Client<TcpStream>, SocketAddr)),
    Message(P, SocketAddr),
    Tick
}

struct TaskDispatcher<P: Packet> {
    channels: Vec<Sender<Task<P>>>,
    channel_addresses: Vec<(SocketAddr, usize)>,
    returns: Receiver<Task<P>>,
    last_pushed: usize
}

impl <P: Packet>TaskDispatcher<P> {
    fn new(returns: Receiver<Task<P>>) -> TaskDispatcher<P> {
        TaskDispatcher { channels: vec![], channel_addresses: vec![], last_pushed: 0, returns }
    }
    fn add_channel(&mut self, channel: Sender<Task<P>>) {
        self.channels.push(channel);
    }
    fn dispatch(&mut self, task: Task<P>) {
        match task {
            Task::Message(ref _msg, addr) => {
                let mut sent = false;
                for (loc, index) in &self.channel_addresses {
                    if addr == *loc {
                        self.channels[*index].send(task).unwrap();
                        sent = true;
                        break;
                    }
                }
                if !sent {
                    panic!();
                }
            }
            Task::TcpConnection((ref _s, addr)) => {
                if self.last_pushed == self.channels.len() - 1 {
                    self.channels[0].send(task).unwrap();
                    self.last_pushed = 0;
                }
                else {
                    self.channels[self.last_pushed + 1].send(task).unwrap();
                    self.last_pushed += 1;
                }
                self.channel_addresses.push((addr, self.last_pushed));
            }
            Task::WsConnection((ref _s, addr)) => {
                if self.last_pushed == self.channels.len() - 1 {
                    self.channels[0].send(task).unwrap();
                    self.last_pushed = 0;
                }
                else {
                    self.channels[self.last_pushed + 1].send(task).unwrap();
                    self.last_pushed += 1;
                }
                self.channel_addresses.push((addr, self.last_pushed));
            }
            _ => {
                if self.last_pushed == self.channels.len() - 1 {
                    self.channels[0].send(task).unwrap();
                    self.last_pushed = 0;
                }
                else {
                    self.channels[self.last_pushed + 1].send(task).unwrap();
                    self.last_pushed += 1;
                }
            }
        }
    }
    fn process_returns(&mut self) {
        while let Ok(task) = self.returns.try_recv() {
            self.dispatch(task);
        }
    }
}
