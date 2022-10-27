use futures::{SinkExt, StreamExt};
use reqwest::StatusCode;
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
  Filter, Future,
};

type Handler = Box<
  dyn Fn(Bytes) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + Sync>>
    + Send
    + Sync,
>;

#[derive(Clone)]
pub struct Socket {
  pub send: mpsc::UnboundedSender<String>,
  pub recieve: flume::Receiver<warp::ws::Message>,
  pub role: String,
}

pub struct ERPCServer {
  /**
   * The port the server runs on
   */
  port: u16,
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
  acitve_sockets: Arc<RwLock<Vec<Socket>>>,
}

impl ERPCServer {
  pub fn new(port: u16) -> ERPCServer {
    return ERPCServer {
      handlers: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
      shutdown_signal: None,
      socket_connected_callbacks: Arc::new(RwLock::new(Vec::new())),
      acitve_sockets: Arc::new(RwLock::new(Vec::new())),
      port
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
    let active_websockets = self.acitve_sockets.clone();
    let connection_callbacks = self.socket_connected_callbacks.clone();

    // handles ws connections which are established with this server to call endpoints on the connected client
    let socket_handler =
      warp::path!("ws" / String)
        .and(warp::ws())
        .and_then(move |role: String, ws: warp::ws::Ws| {
          let active_websockets = active_websockets.clone();
          let connection_callbacks = connection_callbacks.clone();
          async move {
            ws.on_upgrade(|socket| async move {
              let (mut sender, mut reciever) = socket.split();
              // broadcast for incoming ws messages
              let (incoming_sender, incoming_reciever) = flume::unbounded::<warp::ws::Message>();
              // mpsc for outgoing ws messages
              let (outgoing_sender, mut outgoing_reciever) = mpsc::unbounded_channel::<String>();

              tokio::spawn(async move {
                while let Some(message) = reciever.next().await {
                  incoming_sender
                    .send(match message {
                      Ok(v) => v,
                      Err(err) => {
                        eprintln!("Websocket message error: {err}");
                        return;
                      }
                    })
                    .unwrap();
                }
              });

              tokio::spawn(async move {
                while let Some(message) = outgoing_reciever.recv().await {
                  sender.send(warp::ws::Message::text(message)).await.unwrap();
                }
              });

              let mut active_websockets = match active_websockets.write() {
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

              match connection_callbacks.read() {
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
            });

            Ok::<_, Infallible>(warp::reply())
          }
        });

    let callbacks = self.handlers.clone();

    // handles incoming http requests to this server
    let http_request_handler =
      warp::path::full()
        .and(warp::body::bytes())
        .and_then(move |path: FullPath, body| {
          let callbacks = callbacks.clone();
          async move {
            let path = path.as_str();
            if !path.starts_with("/endpoints") {
              Err(warp::reject::not_found())
            } else {
              let result = {
                let lock = callbacks.read().await;
                let path = path.strip_prefix("/endpoints").unwrap();
                let handler = match lock.get(path) {
                  Some(v) => v,
                  None => {
                    eprintln!("Could not find a registered handler for {path}");
                    return Ok(
                      Response::builder()
                        .status(404)
                        .body("Handler not found.".to_string()),
                    );
                  }
                };

                handler(body).await
              };

              match result {
                Ok(v) => Ok(Response::builder().status(200).body(v)),
                Err(err) => {
                  eprintln!("Error while running handler for {path}: {err}");
                  Ok(
                    Response::builder()
                      .status(StatusCode::INTERNAL_SERVER_ERROR)
                      .body("Internal server error. Please see server logs.".to_string()),
                  )
                }
              }
            }
          }
        });

    let (sender, reciever) = oneshot::channel::<()>();
    self.shutdown_signal = Some(sender);

    let (_, server) = warp::serve(socket_handler.or(http_request_handler))
      .bind_with_graceful_shutdown(([127, 0, 0, 1], self.port), async {
        reciever.await.ok();
      });
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
    for socket in match self.acitve_sockets.read() {
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
