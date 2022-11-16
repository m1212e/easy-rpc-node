use std::{
  collections::HashMap,
  sync::{Arc, Mutex},
};

//TODO: error handling could be more elegant

use nanoid::nanoid;
use serde::de::DeserializeOwned;
use tokio::sync::oneshot;

use super::server::{Socket, SocketMessage, SocketRequest, SocketResponse};

#[derive(Debug, Clone)]
pub struct ERPCTarget {
  address: String,
  port: u16,
  target_type: String,
  socket: Arc<Mutex<Option<Socket>>>,
  requests: Arc<Mutex<HashMap<String, oneshot::Sender<SocketResponse>>>>,
  reqwest_client: reqwest::Client,
}

impl ERPCTarget {
  pub fn new(mut address: String, port: u16, target_type: String) -> Self {
    if address.ends_with("/") {
      address.pop();
    }

    ERPCTarget {
      address,
      port,
      target_type,
      socket: Arc::new(Mutex::new(None::<Socket>)),
      requests: Arc::new(Mutex::new(HashMap::new())),
      reqwest_client: reqwest::Client::new(),
    }
  }

  pub async fn call<T: DeserializeOwned>(
    &self,
    method_identifier: String,
    parameters: Vec<impl serde::Serialize>,
  ) -> Result<T, String> {
    if self.target_type == "http-server" {
      let body = match serde_json::to_string(&parameters) {
        Ok(v) => v,
        Err(err) => {
          return Err(format!("Could not serialize: {err}"));
        }
      };

      let r = match self
        .reqwest_client
        .post(format!(
          "{}:{}/endpoints/{method_identifier}",
          self.address, self.port
        ))
        .body(body)
        .send()
        .await
      {
        Ok(v) => v,
        Err(err) => return Err(format!("Request errored: {err}")),
      };

      match r.bytes().await {
        Ok(v) => match serde_json::from_slice::<T>(&v) {
          Ok(v) => Ok(v),
          Err(err) => Err(format!("Error while parsing request body: {err}")),
        },
        Err(err) => Err(format!("Error while awaiting request body: {err}")),
      }
    } else if self.target_type == "browser" {
      let body = match serde_json::to_value(&parameters) {
        Ok(v) => v,
        Err(err) => {
          return Err(format!("Could not serialize: {err}"));
        }
      };

      let socket = match &self.socket.lock() {
        Ok(v) => match &**v {
          Some(v) => v.send.clone(),
          None => {
            return Err(format!("Socket not set for this target!"));
          }
        },
        Err(err) => {
          return Err(format!("Could not access socket lock: {err}"));
        }
      };

      let id = nanoid!();
      let (sender, reciever) = oneshot::channel::<SocketResponse>();
      {
        // scope to drop the requests lock
        let mut requests = match self.requests.lock() {
          Ok(v) => v,
          Err(err) => return Err(format!("Could not access sockets: {err}")),
        };

        requests.insert(id.clone(), sender);
      }

      match socket.send(SocketMessage::Request(SocketRequest {
        id,
        body,
        method_identifier,
      })) {
        Ok(_) => {}
        Err(err) => {
          return Err(format!("Could not send message: {err}"));
        }
      };

      let result = match reciever.await {
        Ok(v) => v,
        Err(err) => return Err(format!("Could not recieve response: {err}")),
      };

      let result = match result.body {
        Some(v) => v,
        None => match result.error {
          Some(err) => return Err(format!("Request failed with: {err}")),
          None => {
            return Err(format!(
              "Invalid response state, request has neither body nor error!"
            ))
          }
        },
      };

      match serde_json::from_value::<T>(result) {
        Ok(v) => Ok(v),
        Err(err) => Err(format!("Error while parsing request body: {err}")),
      }
    } else {
      unreachable!()
    }
  }

  pub async fn listen_on_socket(&mut self, socket: Socket) {
    match self.socket.lock() {
      Ok(mut v) => {
        *v = Some(socket.clone());
      }
      Err(err) => {
        eprintln!("Socket lock error: {err}");
        return;
      }
    }

    loop {
      let msg = match socket.recieve.recv_async().await {
        Ok(v) => v,
        Err(err) => {
          eprintln!("Socket stream error: {err}");
          return;
        }
      };

      match msg {
        SocketMessage::Request(_) => {
          eprintln!("Requests via websocket not supported yet!");
          return;
        }
        SocketMessage::Response(res) => {
          let mut requests = match self.requests.lock() {
            Ok(v) => v,
            Err(err) => {
              eprintln!("Could not access requests (1): {err}");
              return;
            }
          };

          let return_channel = match requests.remove(&res.id) {
            Some(v) => v,
            None => {
              eprintln!("Could not find open request for id {}", res.id);
              return;
            }
          };

          match return_channel.send(res) {
            Ok(_) => {}
            Err(ret_res) => eprintln!("Could not send response for {}", ret_res.id),
          };
        }
      };
    }
  }
}
