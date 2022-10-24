use futures::StreamExt;
use std::{collections::HashMap, pin::Pin, sync::Arc};
use tokio::sync::{oneshot, RwLock};
use warp::{
  hyper::{body::Bytes, Response},
  path::FullPath,
  ws::WebSocket,
  Filter, Future,
};

type Handler = Box<
  dyn Fn(Bytes) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + Sync>>
    + Send
    + Sync,
>;

pub struct ERPCServer {
  /**
   * Request handlers for incoming requests to this server
   */
  handlers: Arc<RwLock<HashMap<String, Handler>>>,
  /**
   * Shutdown signal to exit the webserver gracefully
   */
  shutdown_signal: Option<oneshot::Sender<()>>,
  /**
   * Callbacks which want to be notified of new websocket connections to this server
   */
  socket_connected_callbacks: Arc<RwLock<Vec<Box<dyn Fn(String, WebSocket)>>>>,
  /**
   * Websockets which are currently connected to this server
   */
  acitve_sockets: Arc<RwLock<Vec<WebSocket>>>,
}

impl ERPCServer {
  pub fn new() -> ERPCServer {
    return ERPCServer {
      handlers: Arc::new(RwLock::new(HashMap::new())),
      shutdown_signal: None,
      socket_connected_callbacks: Arc::new(RwLock::new(Vec::new())),
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

  pub async fn run(&mut self, port: u16) -> Result<(), String> {
    let callbacks = self.handlers.clone();

    // handles ws connections which are established with this server to call endpoints on the connected client
    let socket_handler =
      warp::path!("ws" / String)
        .and(warp::ws())
        .map(|role, ws: warp::ws::Ws| {
          ws.on_upgrade(|socket| async move {
            let (tx, rx) = socket.split();
          })
        });

    // handles incoming http requests to this server
    let http_request_handler =
      warp::path::full()
        .and(warp::body::bytes())
        .and_then(move |path: FullPath, body| {
          let callbacks = callbacks.clone();
          async move {
            let path = path.as_str();
            if path == "/" {
              Err(warp::reject::not_found())
            } else {
              let result = {
                let lock = callbacks.read().await;

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
                      .status(400)
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
      .bind_with_graceful_shutdown(([127, 0, 0, 1], port), async {
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
}
