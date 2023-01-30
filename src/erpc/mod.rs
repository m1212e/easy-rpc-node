pub mod handler;
pub mod protocol;
pub mod server;
pub mod target;

#[derive(Clone, Debug)]
pub struct Socket {
  pub sender: flume::Sender<protocol::socket::SocketMessage>,
  pub reciever: flume::Receiver<protocol::socket::SocketMessage>,
  pub role: String,
}
