use bytes::Bytes;
use futures::{Future, SinkExt, StreamExt};
use hyper::http::HeaderValue;
use reqwest::{StatusCode};
use std::{
  collections::{HashMap, HashSet},
  net::SocketAddr,
  pin::Pin,
  sync::{Arc, RwLock},
};
use tokio::sync::{mpsc, oneshot};
use viz::{
  middleware::cors,
  types::{Params, WebSocket},
  Request, RequestExt, Response, Router, Server, ServiceMaker,
};

//TODO: include in docs that credentials are sent by default
//TODO: refactor socket for easier usage

use serde::{Deserialize, Serialize};

type Handler = Box<
  dyn Fn(Bytes) -> Pin<Box<dyn Future<Output = Result<Bytes, String>> + Send + Sync>> + Send + Sync,
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
   * If this server should accept websocket connections
   */
  enable_sockets: bool,
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

#[derive(Clone)]
struct EPRCHandler {
  /**
   * Request handlers for incoming requests to this server
   */
  handlers: Arc<tokio::sync::RwLock<HashMap<String, Handler>>>,
}

impl viz::Handler<Request> for EPRCHandler {
  type Output = viz::Result<Response>;

  async fn call(&self, req: Request) -> Self::Output {
      let path = req.path().clone();
      let method = req.method().clone();
      let count = self.count.fetch_add(1, Ordering::SeqCst);
      Ok(format!("method = {method}, path = {path}, count = {count}").into_response())
  }
}

impl ERPCServer {
  pub fn new(port: u16, allowed_cors_origins: Vec<String>, enable_sockets: bool) -> ERPCServer {
    return ERPCServer {
      handlers: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
      shutdown_signal: None,
      enable_sockets,
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
    let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
    println!("🚀 easy-rpc listening on {addr}...");

    let app = Router::new();

    app.with(cors::Config {
      allow_origins: HashSet::from(
        self
          .allowed_cors_origins
          .iter()
          .map(|v| HeaderValue::from_str(v))
          .collect(),
      ),
      credentials: true,
      ..Default::default()
    });

    let handlers = self.handlers.clone();
    app.any("/endpoints", |req| http_handler(req, handlers));

    // app.any("/endpoints", );

    // if self.enable_sockets {
    //   app.any("/ws/:role", socket_handler(req, active_sockets, socket_connected_callbacks));
    // }

    // shutdown signal
    let (sender, reciever) = oneshot::channel::<()>();
    self.shutdown_signal = Some(sender);

    if let Err(e) = Server::bind(&addr)
      .serve(ServiceMaker::from(app))
      .with_graceful_shutdown(async { reciever.await.ok().unwrap() })
      .await
    {
      return Err(format!("{:#?}", e));
    }

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
  req: Request,
  request_handlers: Arc<tokio::sync::RwLock<HashMap<String, Handler>>>,
) -> viz::Result<Response> {
  let path = req
    .uri()
    .path()
    .strip_prefix("/endpoints/")
    .unwrap()
    .to_owned();

  let result = {
    let lock = request_handlers.read().await;

    let handler = match lock.get(&path) {
      Some(v) => v,
      None => {
        eprintln!("Could not find a registered handler for {path}");
        return Ok(
          Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(viz::Body::from("Handler not found."))
            .unwrap(),
        );
      }
    };

    let body = match hyper::body::to_bytes(req.into_body()).await {
      Ok(v) => v,
      Err(err) => {
        eprintln!("Could not extract body from request {err}");
        return Ok(
          Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(viz::Body::from("Please see the server logs."))
            .unwrap(),
        );
      }
    };

    handler(body).await
  };

  match result {
    Ok(v) => {
      return Ok(
        Response::builder()
          .status(StatusCode::OK)
          .body(v.into())
          .unwrap(),
      );
    }
    Err(err) => {
      eprintln!("Error while running handler for {path}: {err}");
      return Ok(
        Response::builder()
          .status(StatusCode::INTERNAL_SERVER_ERROR)
          .body(viz::Body::from("Please see the server logs."))
          .unwrap(),
      );
    }
  }
}

async fn socket_handler(
  mut req: Request,
  active_sockets: Arc<RwLock<Vec<Socket>>>,
  socket_connected_callbacks: Arc<RwLock<Vec<Box<dyn Fn(&Socket) + Send + Sync>>>>,
) -> viz::Result<Response> {
  let (ws, Params(role)): (WebSocket, Params<String>) = match req.extract().await {
    Ok(v) => v,
    Err(err) => todo!(),
  };

  Ok(ws.on_upgrade(move |socket| async move {
    let (mut socket_sender, mut socket_reciever) = socket.split();
    // broadcast for incoming ws messages
    let (incoming_sender, incoming_reciever) = flume::unbounded::<SocketMessage>();
    // mpsc for outgoing ws messages
    let (outgoing_sender, mut outgoing_reciever) = mpsc::unbounded_channel::<SocketMessage>();

    tokio::spawn(async move {
      while let Some(message) = socket_reciever.next().await {
        let message = match message {
          Ok(v) => {
            let message_parse_result = match v {
              viz::types::Message::Text(text) => serde_json::from_str::<SocketMessage>(&text),
              viz::types::Message::Binary(binary) => {
                serde_json::from_slice::<SocketMessage>(&binary)
              }
              viz::types::Message::Ping(msg) => continue,
              viz::types::Message::Close(_) => break,
              viz::types::Message::Pong(_) => continue,
              viz::types::Message::Frame(_) => continue,
            };

            let message: SocketMessage = match message_parse_result {
              Ok(v) => v,
              Err(err) => {
                eprintln!("Websocket message parse error: {err}");
                return;
              }
            };
            message
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
          .send(viz::types::Message::Text(text))
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
  }))
}
