use futures::{FutureExt, SinkExt, StreamExt};
use reqwest::{Method, StatusCode};
use std::{
  collections::HashMap,
  convert::Infallible,
  pin::Pin,
  sync::{Arc, RwLock},
};
use tokio::sync::{mpsc, oneshot};
use warp::{
  hyper::{body::Bytes, Response},
  path::FullPath,
  Filter, Future, Reply,
};

//TODO: include in docs that credentials are sent by default
//TODO: refactor socket for easier usage

use serde::{Deserialize, Serialize};

type Handler = Box<
  dyn Fn(String) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + Sync>>
    + Send
    + Sync,
>;

#[derive(Clone, Debug)]
pub struct Socket {
  pub send: mpsc::UnboundedSender<SocketMessage>,
  pub recieve: flume::Receiver<SocketMessage>,
  pub role: String,
}

//TODO: check if Strings are the best way to pass the body data around

#[derive(Serialize, Deserialize, Debug)]
pub struct SocketRequest {
  pub id: String,
  pub method_identifier: String,
  pub body: serde_json::Value,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SocketResponse {
  pub id: String,
  pub body: Option<serde_json::Value>,
  pub error: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum SocketMessage {
  Request(SocketRequest),
  Response(SocketResponse),
}

//TODO: check where rwlock/mutex is necessary
pub struct ERPCServer {
  /**
   * The port the server runs on
   */
  port: u16,
  /**
   * List of the allowed origins
   */
  allowed_cors_origins: Vec<String>,
  /**
   * Request handlers for incoming requests to this server
   */
  handlers: Arc<tokio::sync::RwLock<HashMap<String, Handler>>>,
  /**
   * Shutdown signal to exit the webserver gracefully
   */
  shutdown_signal: Option<oneshot::Sender<()>>,
  /**
   * Callbacks which want to be notified of new websocket connections to this server
   */
  socket_connected_callbacks: Arc<RwLock<Vec<Box<dyn Fn(&Socket) + Send + Sync>>>>,
  /**
   * Websockets which are currently connected to this server
   */
  active_sockets: Arc<RwLock<Vec<Socket>>>,
}

impl ERPCServer {
  pub fn new(port: u16, allowed_cors_origins: Vec<String>) -> ERPCServer {
    return ERPCServer {
      handlers: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
      shutdown_signal: None,
      socket_connected_callbacks: Arc::new(RwLock::new(Vec::new())),
      active_sockets: Arc::new(RwLock::new(Vec::new())),
      port,
      allowed_cors_origins,
    };
  }

  pub fn register_handler(&mut self, handler: Handler, identifier: &str) -> Result<(), String> {
    self
      .handlers
      //TODO: should this become async and not use blocking:write?
      .blocking_write()
      .insert(identifier.to_owned(), handler);

    Ok(())
  }

  pub async fn run(&mut self) -> Result<(), String> {
    let active_sockets = self.active_sockets.clone();
    let socket_connected_callbacks = self.socket_connected_callbacks.clone();
    let handlers = self.handlers.clone();

    // handles ws connections which are established with this server to call endpoints on the connected client
    let socket_handler =
      warp::path!("ws" / String)
        .and(warp::ws())
        .map(move |role, ws: warp::ws::Ws| {
          let active_sockets = active_sockets.clone();
          let socket_connected_callbacks = socket_connected_callbacks.clone();

          socket_handler(role, ws, active_sockets, socket_connected_callbacks)
        });

    // handles incoming http requests to this server
    let http_request_handler =
      warp::path::full()
        .and(warp::body::bytes())
        .and_then(move |path, body| {
          let handlers = handlers.clone();
          async { Ok::<_, Infallible>(http_handler(path, body, handlers).await) }
        });

    // sets CORS headers to outgoing requests
    let mut cors = warp::cors();

    if self.allowed_cors_origins.contains(&"*".to_string()) {
      cors = cors.allow_any_origin();
    } else {
      for origin in &self.allowed_cors_origins {
        cors = cors.allow_origin(origin.as_str());
      }
    }
    cors = cors.allow_methods(vec![Method::GET, Method::POST]);
    cors = cors.allow_credentials(true);

    let (sender, reciever) = oneshot::channel::<()>();
    self.shutdown_signal = Some(sender);

    let combined_filter = (socket_handler.or(http_request_handler)).with(cors);

    let (_, server) = warp::serve(combined_filter).bind_with_graceful_shutdown(
      ([127, 0, 0, 1], self.port),
      async {
        reciever.await.ok();
      },
    );

    println!("Listening on {}", self.port);
    server.await;

    Ok(())
  }

  pub fn stop(&mut self) -> Result<(), String> {
    let sender = match &self.shutdown_signal {
      Some(_) => std::mem::replace(&mut self.shutdown_signal, None).unwrap(),
      None => {
        return Err("Server can't be stopped if not started!".to_string());
      }
    };

    match sender.send(()) {
      Ok(_) => {}
      Err(err) => {
        return Err(format!(
          "Server can't be stopped because of error: {:#?}",
          err
        ));
      }
    };

    Ok(())
  }

  pub fn register_socket_connection_callback(
    &mut self,
    cb: Box<dyn Fn(&Socket) + Send + Sync>,
  ) -> Result<(), String> {
    for socket in match self.active_sockets.read() {
      Ok(v) => v,
      Err(err) => {
        return Err(format!("Could not register socket callback (1): {err}"));
      }
    }
    .iter()
    {
      cb(socket)
    }

    let mut cbs = match self.socket_connected_callbacks.write() {
      Ok(v) => v,
      Err(err) => {
        return Err(format!("Could not register socket callback (2): {err}"));
      }
    };
    cbs.push(cb);

    Ok(())
  }
}

async fn http_handler(
  path: FullPath,
  body: Bytes,
  request_handlers: Arc<tokio::sync::RwLock<HashMap<String, Handler>>>,
) -> warp::http::Response<String> {
  let path = path.as_str();
  if !path.starts_with("/endpoints") {
    return Response::builder()
      .status(StatusCode::NOT_FOUND)
      .body("Use /ws or /endpoints".to_string())
      .unwrap();
  }

  let result = {
    let lock = request_handlers.read().await;
    let path = path.strip_prefix("/endpoints/").unwrap();
    let handler = match lock.get(path) {
      Some(v) => v,
      None => {
        eprintln!("Could not find a registered handler for {path}");
        return Response::builder()
          .status(StatusCode::NOT_FOUND)
          .body("Handler not found.".to_string())
          .unwrap();
      }
    };

    let str = match String::from_utf8(body.to_vec()) {
      Ok(v) => v,
      Err(err) => {
        eprintln!("Could not convert body to string {err}");
        return Response::builder()
          .status(StatusCode::INTERNAL_SERVER_ERROR)
          .body("Internal server error. Please see server logs.".to_string())
          .unwrap();
      }
    };

    handler(str).await
  };

  match result {
    Ok(v) => Response::builder().status(StatusCode::OK).body(v).unwrap(),
    Err(err) => {
      eprintln!("Error while running handler for {path}: {err}");
      Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body("Internal server error. Please see server logs.".to_string())
        .unwrap()
    }
  }
}

fn socket_handler(
  role: String,
  ws: warp::ws::Ws,
  active_sockets: Arc<RwLock<Vec<Socket>>>,
  socket_connected_callbacks: Arc<RwLock<Vec<Box<dyn Fn(&Socket) + Send + Sync>>>>,
) -> impl Reply {
  ws.on_upgrade(|socket| async move {
    let (mut socket_sender, mut socket_reciever) = socket.split();
    // broadcast for incoming ws messages
    let (incoming_sender, incoming_reciever) = flume::unbounded::<SocketMessage>();
    // mpsc for outgoing ws messages
    let (outgoing_sender, mut outgoing_reciever) = mpsc::unbounded_channel::<SocketMessage>();


    tokio::spawn(async move {
      while let Some(message) = socket_reciever.next().await {
        let message = match message {
          Ok(v) => {
            let m: SocketMessage = match serde_json::from_slice(&v.as_bytes()) {
              Ok(v) => v,
              Err(err) => {
                eprintln!("Websocket message parse error: {err}");
                return;
              }
            };
            m
          }
          Err(err) => {
            eprintln!("Websocket message error: {err}");
            return;
          }
        };

        incoming_sender.send(message).unwrap();
      }
    });

    tokio::spawn(async move {
      while let Some(message) = outgoing_reciever.recv().await {
        let text = match serde_json::to_string(&message) {
          Ok(v) => v,
          Err(err) => {
            eprintln!("Could not serialize ws message: {err}");
            return;
          }
        };
        socket_sender
          .send(warp::ws::Message::text(text))
          .await
          .unwrap();
      }
    });

    let mut active_websockets = match active_sockets.write() {
      Ok(v) => v,
      Err(err) => {
        println!("PoisonError in active_websockets lock: {err}");
        return;
      }
    };

    active_websockets.push(Socket {
      send: outgoing_sender.clone(),
      recieve: incoming_reciever.clone(),
      role: role.clone(),
    });

    match socket_connected_callbacks.read() {
      Ok(v) => {
        for cb in v.iter() {
          cb(&Socket {
            send: outgoing_sender.clone(),
            recieve: incoming_reciever.clone(),
            role: role.clone(),
          });
        }
      }
      Err(err) => eprintln!("Could not read connection callbacks: {err}"),
    }
  })
}
