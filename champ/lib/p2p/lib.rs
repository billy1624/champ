use crate::connection::Connection;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

mod connection;

#[allow(dead_code)]
struct P2pClient {
    config: Arc<P2pConfig>,
    connections: Arc<Mutex<Vec<Connection>>>,
}

#[allow(dead_code)]
impl P2pClient {
    pub fn new(config: P2pConfig) -> P2pClient {
        P2pClient {
            config: Arc::new(config),
            connections: Arc::new(Mutex::new(vec![])),
        }
    }

    #[allow(unreachable_code)]
    pub async fn start(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let connections = self.connections.clone();
        let config = self.config.clone();

        let handle = tokio::spawn(async move {
            let listener = TcpListener::bind(&config.listening_port).await?;
            //wait for connections and only accept when below the limit
            //make this more efficient by starting to listen only when below the limit
            loop {
                let mut connections = connections.lock().await;
                if connections.len() >= usize::from(config.max_connections) {
                    sleep(Duration::from_millis(1000)).await;
                    continue;
                }
                let connection = Connection::accept(&listener).await.expect("Could'nt accept connection");
                connections.push(connection)
            }
            Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
        });

        let _result = handle.await?;
        Ok(())
    }
}

#[allow(dead_code)]
//holds different config values for the P2pClient
pub struct P2pConfig {
    max_connections: u16,
    max_incoming_connections: u16,
    listening_port: &'static str,
    is_delegate: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[tokio::test]
    #[serial]
    //test passes when TcpStream::connect yields a TcpStream value and therefore succeeds
    async fn listen_for_connections() {
        let max_number_connections = 5;
        let listening_port = "127.0.0.1:7899";
        let config = P2pConfig {
            max_connections: max_number_connections,
            max_incoming_connections: max_number_connections,
            listening_port,
            is_delegate: false,
        };

        tokio::spawn(async move {
            let client = P2pClient::new(config);
            client.start().await.expect("failed listening for the connection");
        });

        let mut list = vec![];
        for i in 0..max_number_connections {
            let stream = tokio::spawn(async move {
                Connection::connect(listening_port.clone()).await.expect(&*format!("failed connecting Nr. {}", i))
            });
            list.push(stream.await.expect("expected TcpStream"));
        }

        assert!(list.len() == max_number_connections.into())
    }
}
