use std::net::{SocketAddr, TcpListener};

use super::ServerConfig;
use crate::{LOCAL_ADDRESS, GLOBAL_ADDRESS};
use websocket::{sync::*, server::{WsServer, NoTlsAcceptor}};

pub fn listener<P, G>(config: ServerConfig<P, G>) -> WsServer<NoTlsAcceptor, TcpListener> {
    let address = if config.public_facing {
        GLOBAL_ADDRESS
    }
    else {
        LOCAL_ADDRESS
    };
    match Server::bind(SocketAddr::from((address, config.ws_port))) {
        Ok(listener) => {
            listener.set_nonblocking(true).unwrap();
            return listener;
        }
        Err(error) => {
            panic!("{error}");
        }
    }
}
