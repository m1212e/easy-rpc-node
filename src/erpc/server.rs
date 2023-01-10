use futures_util::{Future, SinkExt, StreamExt};
use reqwest::{Method, StatusCode};
use std::{
  collections::HashMap,
  pin::Pin,
  sync::{Arc, RwLock},
};
use tokio::sync::{mpsc, oneshot};
use warp::{path::Peek, Filter, Reply};

//TODO: include in docs that credentials are sent by default
//TODO: refactor socket for easier usage

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::handler::Handler as HandlerTrait;

type Handler = Box<
  dyn Fn(
      serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, String>> + Send + Sync>>
    + Send
    + Sync,
>;

type SocketCallbacks = Arc<RwLock<Vec<Box<dyn Fn(&Socket) + Send + Sync>>>>;

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
#[derive(Clone)]
pub struct ERPCServer {
  /**
   * The port the server runs on
   */
  port: u16,
  /**
   * Whether the server should accept websocket connections
   */
  enabled_sockets: bool,
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
  shutdown_signal: Arc<RwLock<Option<oneshot::Sender<()>>>>,
  /**
   * Callbacks which want to be notified of new websocket connections to this server
   */
  socket_connected_callbacks: SocketCallbacks,
  /**
   * Websockets which are currently connected to this server
   */
  active_sockets: Arc<RwLock<Vec<Socket>>>,
}

impl ERPCServer {
  pub fn new(port: u16, allowed_cors_origins: Vec<String>, enabled_sockets: bool) -> Self {
    ERPCServer {
      handlers: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
      shutdown_signal: Arc::new(RwLock::new(None)),
      socket_connected_callbacks: Arc::new(RwLock::new(Vec::new())),
      active_sockets: Arc::new(RwLock::new(Vec::new())),
      port,
      allowed_cors_origins,
      enabled_sockets,
    }
  }

  #[allow(dead_code)]
  pub fn register_raw_handler(&mut self, handler: Handler, identifier: &str) {
    self
      .handlers
      //TODO: should this become async and not use blocking:write?
      .blocking_write()
      .insert(identifier.to_owned(), handler);
  }

  #[allow(dead_code)]
  pub fn register_handler<H, P>(&mut self, handler: H, identifier: &str)
  where
    P: DeserializeOwned + Send + Sync,
    H: super::handler::Handler<P> + 'static,
    H::Output: Serialize,
  {
    let v: Handler = Box::new(move |v| {
      let handler = handler.clone();
      Box::pin(async move {
        let parameters = match serde_json::from_value::<P>(v) {
          Ok(v) => v,
          Err(err) => {
            return Err(format!("Failed to parse parameters: {}", err));
          }
        };

        let result = handler.run(parameters).await?;

        let serialized = match serde_json::to_value(result) {
          Ok(v) => v,
          Err(err) => {
            return Err(format!("Failed to serialize result: {}", err));
          }
        };

        Ok(serialized)
      })
    });

    self
      .handlers
      //TODO: should this become async and not use blocking:write?
      .blocking_write()
      .insert(identifier.to_owned(), v);
  }

  pub fn run(&self) -> Result<impl futures_util::Future<Output = ()> + Send + Sync, String> {
    let active_sockets = self.active_sockets.clone();
    let socket_connected_callbacks = self.socket_connected_callbacks.clone();
    let handlers = self.handlers.clone();
    let enabled_sockets = self.enabled_sockets;

    let active_sockets = warp::any().map(move || active_sockets.clone());
    let socket_connected_callbacks = warp::any().map(move || socket_connected_callbacks.clone());
    let handlers = warp::any().map(move || handlers.clone());
    let enabled_sockets = warp::any().map(move || enabled_sockets);

    let mut cors = warp::cors()
      .allow_methods(vec![Method::GET, Method::POST])
      .allow_credentials(true);

    if self.allowed_cors_origins.contains(&"*".to_string()) {
      cors = cors.allow_any_origin();
    } else {
      for origin in &self.allowed_cors_origins {
        cors = cors.allow_origin(origin.as_str());
      }
    }

    let http = warp::path!("endpoints" / ..)
      .and(handlers)
      .and(warp::path::peek())
      .and(warp::body::json())
      .and(warp::body::content_length_limit(1024 * 64))
      .then(Self::http_handler)
      .with(cors.clone());

    let handlers = self.handlers.clone();
    let request_handlers = warp::any().map(move || handlers.clone());
    let ws = warp::path!("ws" / String)
      .and(enabled_sockets)
      .and(active_sockets)
      .and(request_handlers)
      .and(socket_connected_callbacks)
      .and(warp::ws())
      .map(
        |role,
         enabled_sockets,
         active_sockets,
         request_handlers,
         socket_connected_callbacks,
         ws| {
          Self::socket_handler(
            role,
            enabled_sockets,
            active_sockets,
            request_handlers,
            socket_connected_callbacks,
            ws,
          )
        },
      )
      .with(cors.clone());

    let (sender, reciever) = oneshot::channel::<()>();
    self
      .shutdown_signal
      .write()
      .map_err(|err| format!("Could not set shutdown signal: {err}"))?
      .replace(sender);

    let (_, server) = warp::serve(http.or(ws).with(cors)).bind_with_graceful_shutdown(
      ([127, 0, 0, 1], self.port),
      async {
        reciever.await.ok();
      },
    );

    Ok(server)
  }

  pub fn stop(&self) -> Result<(), String> {
    let mut w = self
      .shutdown_signal
      .write()
      .map_err(|err| format!("Could not set shutdown signal: {err}"))?;
    let sender = match w.take() {
      Some(v) => v,
      None => {
        return Err("Server is not running".to_string());
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

  async fn http_handler(
    request_handlers: Arc<tokio::sync::RwLock<HashMap<String, Handler>>>,
    path: Peek,
    parameters: serde_json::Value,
  ) -> Box<dyn Reply> {
    let lock = request_handlers.read().await;
    let handler = match lock.get(path.as_str()) {
      Some(v) => v,
      None => {
        eprintln!("Could not find a registered handler for {}", path.as_str());
        return Box::new(warp::reply::with_status(
          "Internal server error. Please see server logs",
          StatusCode::NOT_FOUND,
        ));
      }
    };

    let parameters = match handler.deserialize_parameters(parameters) {
      Ok(v) => v,
      Err(err) => {
        eprintln!(
          "Could not deserialize parameters: {err} for {}",
          path.as_str()
        );
        return Box::new(warp::reply::with_status(
          "Internal server error. Please see server logs",
          StatusCode::NOT_FOUND,
        ));
      }
    };

    let result = match handler.run(parameters).await {
      Ok(v) => v,
      Err(err) => {
        eprintln!("Error while running handler {}: {err}", path.as_str());
        return Box::new(warp::reply::with_status(
          "Internal server error. Please see server logs",
          StatusCode::INTERNAL_SERVER_ERROR,
        ));
      }
    };

    Box::new(warp::reply::json(&result))
  }

  fn socket_handler(
    role: String,
    enabled_sockets: bool,
    active_sockets: Arc<RwLock<Vec<Socket>>>,
    _request_handlers: Arc<tokio::sync::RwLock<HashMap<String, Handler>>>, // in the future we might also handle requests incoming via sockets
    socket_connected_callbacks: SocketCallbacks,
    ws: warp::ws::Ws,
  ) -> Box<dyn Reply> {
    if enabled_sockets {
      Box::new(ws.on_upgrade(|socket| async move {
        let (mut socket_sender, mut socket_reciever) = socket.split();
        // broadcast for incoming ws messages
        let (incoming_sender, incoming_reciever) = flume::unbounded::<SocketMessage>();
        // mpsc for outgoing ws messages
        let (outgoing_sender, mut outgoing_reciever) = mpsc::unbounded_channel::<SocketMessage>();

        tokio::spawn(async move {
          while let Some(message) = socket_reciever.next().await {
            let message = match message {
              Ok(v) => {
                let m: SocketMessage = match serde_json::from_slice(v.as_bytes()) {
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
            eprintln!("PoisonError in active_websockets lock: {err}");
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
    } else {
      Box::new(warp::reply::with_status(
        "Websockets are disabled",
        StatusCode::NOT_IMPLEMENTED,
      ))
    }
  }
}
