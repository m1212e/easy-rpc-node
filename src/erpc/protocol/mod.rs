pub mod socket;

use serde::{Deserialize, Serialize};
use vec1::Vec1;

/**
   In incoming erpc request.
   When no parameters are sent, the option is none instead of an empty vec.
*/
#[derive(Serialize, Deserialize, Debug)]
pub struct Request {
  pub identifier: String,
  pub parameters: Vec1<serde_json::Value>,
}

/**
   An outgoing erpc response
*/
#[derive(Serialize, Deserialize, Debug)]
pub struct Response {
  pub body: serde_json::Value,
}
