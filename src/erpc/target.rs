// use std::{
//   collections::HashMap,
//   sync::{Arc, RwLock},
// };

// use tokio::sync::oneshot;
// use warp::hyper::body::Bytes;

// use super::server::Socket;

// pub struct ERPCTarget {
//   adress: String,
//   port: u16,
//   types: Vec<String>,
//   socket: Option<Socket>,
//   requests: Arc<RwLock<HashMap<String, oneshot::Sender<String>>>>,
//   reqwest_client: reqwest::Client,
// }

// impl ERPCTarget {
//   pub fn new(mut adress: String, port: u16, types: Vec<String>) -> Self {
//     if adress.ends_with("/") {
//       adress.pop();
//     }

//     ERPCTarget {
//       adress,
//       port,
//       types,
//       socket: None,
//       requests: Arc::new(RwLock::new(HashMap::new())),
//       reqwest_client: reqwest::Client::new(),
//     }
//   }

//   pub async fn call(
//     &self,
//     method_identifier: String,
//     parameters: Vec<impl serde::Serialize>,
//   ) -> Result<Bytes, String> {
//     let body = match serde_json::to_string(&parameters) {
//       Ok(v) => v,
//       Err(err) => {
//         return Err(format!("Could not serialize: {err}"));
//       }
//     };

//     if self.types.contains(&"http-server".to_string()) {
//       let r = match self
//         .reqwest_client
//         .post(format!(
//           "{}:{}/endpoints{method_identifier}",
//           self.adress, self.port
//         ))
//         .body(body)
//         .send()
//         .await
//       {
//         Ok(v) => v,
//         Err(err) => return Err(format!("Request errored: {err}")),
//       };

//       Ok(r.bytes().await);
//     };

//     Ok(Bytes::default())
//   }
// }
