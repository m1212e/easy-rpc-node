use super::{protocol::socket::SocketMessage, Socket};
use nanoid::nanoid;
use serde::{de::DeserializeOwned, Serialize};
use std::{
  collections::HashMap,
  fmt::Debug,
  sync::{Arc, Mutex},
};
use tokio::sync::oneshot;

#[derive(Debug, Clone)]
pub enum TargetType {
  HTTPServer,
  Browser,
}

//TODO find a better/faster way to store open requests
#[derive(Debug, Clone)]
pub struct ERPCTarget {
  address: String,
  port: u16,
  target_type: TargetType,
  socket: Arc<Mutex<Option<Socket>>>,
  requests: Arc<Mutex<HashMap<String, oneshot::Sender<super::protocol::socket::Response>>>>,
  reqwest_client: reqwest::Client,
}

impl ERPCTarget {
  pub fn new(mut address: String, port: u16, target_type: TargetType) -> Self {
    if address.ends_with('/') {
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

  pub async fn call<P: Serialize, R: DeserializeOwned + Debug>(
    &self,
    identifier: String,
    parameters: Vec<P>,
  ) -> Result<R, String> {
    // making sure that the protocol::Request is used to break this if the protocol should ever change
    let request = crate::erpc::protocol::Request {
      identifier,
      parameters: parameters
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("Could not parse parameters: {err}"))?,
    };

    match self.target_type {
      TargetType::HTTPServer => {
        let r = self
          .reqwest_client
          .post(format!(
            "{}:{}/handlers/{}",
            self.address, self.port, request.identifier
          ))
          .header("Content-Type", "application/json")
          .body(serde_json::to_vec(&request.parameters).expect("Vec of json::Value should be ok"));

        let response = r
          .send()
          .await
          .map_err(|err| format!("Request errored: {err}"))?
          .bytes()
          .await
          .map_err(|err| format!("Error while awaiting request body: {err}"))?;

        serde_json::from_slice(&response)
          .map_err(|err| format!("Could not deserialize response: {err}"))
      }
      TargetType::Browser => {
        let socket = {
          let socket = self
            .socket
            .lock()
            .map_err(|err| format!("Could not lock socket mutex: {err}"))?;

          match &*socket {
            Some(v) => v.clone(),
            None => return Err("Socket not set for this target".to_string()),
          }
        };

        let id = nanoid!();
        let (sender, reciever) = oneshot::channel::<super::protocol::socket::Response>();
        {
          // scope to drop the requests lock
          let mut requests = self
            .requests
            .lock()
            .map_err(|err| format!("Could not access sockets: {err}"))?;

          requests.insert(id.clone(), sender);
        }

        socket
          .sender
          .send(SocketMessage::Request(super::protocol::socket::Request {
            id,
            request,
          }))
          .unwrap();

        let response = reciever
          .await
          .map_err(|err| format!("RecvError in socket response channel: {err}"))?;

        let response = response.body?;

        serde_json::from_value(response.body)
          .map_err(|err| format!("Could not parse socket response: {err}"))?
      }
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
      let msg = match socket.reciever.recv_async().await {
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
