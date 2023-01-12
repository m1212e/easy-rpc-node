use serde::{Deserialize, Serialize};

/**
   In incoming erpc request.
   When no parameters are sent, the option is none instead of an empty vec.
*/
#[derive(Serialize, Deserialize, Debug)]
pub struct Request<'a> {
  pub identifier: &'a str,
  pub parameters: &'a Option<Vec<serde_json::Value>>,
}

/**
   An outgoing erpc response
*/
#[derive(Serialize, Deserialize, Debug)]
pub struct Response<'a> {
  pub body: &'a Option<serde_json::Value>,
}

/**
   A request via websockets
*/
#[derive(Serialize, Deserialize, Debug)]
pub struct SocketRequest<'a> {
  pub id: &'a str,
  pub request: &'a Request<'a>,
}

/*
    A response to a websocket request
*/
#[derive(Serialize, Deserialize, Debug)]
pub struct SocketResponse<'a> {
  pub id: &'a str,
  pub body: Result<Response<'a>, &'a str>,
}
