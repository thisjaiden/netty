use std::net::{TcpListener, SocketAddr};

use crate::{LOCAL_ADDRESS, GLOBAL_ADDRESS};

use super::ServerConfig;

pub fn listener<P, G>(config: ServerConfig<P, G>) -> TcpListener {
    let address = if config.public_facing {
        GLOBAL_ADDRESS
    }
    else {
        LOCAL_ADDRESS
    };
    match TcpListener::bind(SocketAddr::from((address, config.tcp_port))) {
        Ok(listener) => {
            listener.set_nonblocking(true).unwrap();
            return listener;
        }
        Err(error) => {
            panic!("{error}");
        }
    }
}
