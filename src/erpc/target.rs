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
  adress: String,
  port: u16,
  types: Vec<String>,
  socket: Option<Socket>,
  requests: Arc<Mutex<HashMap<String, oneshot::Sender<SocketResponse>>>>,
  reqwest_client: reqwest::Client,
}

impl ERPCTarget {
  pub fn new(mut adress: String, port: u16, types: Vec<String>) -> Self {
    if adress.ends_with("/") {
      adress.pop();
    }

    ERPCTarget {
      adress,
      port,
      types,
      socket: None::<Socket>,
      requests: Arc::new(Mutex::new(HashMap::new())),
      reqwest_client: reqwest::Client::new(),
    }
  }

  pub async fn call<T: DeserializeOwned>(
    &self,
    method_identifier: String,
    parameters: Vec<impl serde::Serialize>,
  ) -> Result<T, String> {
    let body = match serde_json::to_string(&parameters) {
      Ok(v) => v,
      Err(err) => {
        return Err(format!("Could not serialize: {err}"));
      }
    };

    if self.types.contains(&"http-server".to_string()) {
      let r = match self
        .reqwest_client
        .post(format!(
          "{}:{}/endpoints/{method_identifier}",
          self.adress, self.port
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
    } else if self.types.contains(&"browser".to_string()) {
      let socket = match &self.socket {
        Some(v) => v.send.clone(),
        None => {
          return Err(format!("Socket not set for this target!"));
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

      match serde_json::from_str::<T>(&result) {
        Ok(v) => Ok(v),
        Err(err) => Err(format!("Error while parsing request body: {err}")),
      }
    } else {
      unreachable!()
    }
  }

  pub async fn listen_on_socket(&mut self, socket: Socket) {
    self.socket = Some(socket.clone());

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
