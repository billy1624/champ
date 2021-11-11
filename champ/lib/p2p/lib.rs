mod connection;

#[allow(dead_code)]
struct P2pClient {
    config: P2pConfig
}

#[allow(dead_code)]
impl P2pClient {
    pub fn new(&self, config: P2pConfig) -> P2pClient {
        P2pClient { config }
    }

    pub async fn start(&mut self) {
        //do starting stuff
    }
}

#[allow(dead_code)]
//holds different config values for the P2pClient
pub struct P2pConfig {
    max_number_connections: i32,
    max_incoming_connections: i32,
    listening_port: String,
    is_delegate: bool,
}