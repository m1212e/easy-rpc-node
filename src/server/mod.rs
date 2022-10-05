use std::{collections::HashMap, pin::Pin, sync::Arc};
use tokio::sync::{oneshot, RwLock};
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

pub struct ERPCServer {
  callbacks: Arc<RwLock<HashMap<String, Handler>>>,
  shutdown_signal: Option<oneshot::Sender<()>>,
}

impl ERPCServer {
  pub fn new() -> ERPCServer {
    return ERPCServer {
      callbacks: Arc::new(RwLock::new(HashMap::new())),
      shutdown_signal: None,
    };
  }

  pub fn register_handler(&mut self, handler: Handler, identifier: &str) -> Result<(), String> {
    self
      .callbacks
      //TODO: should this become async and not use blocking:write?
      .blocking_write()
      .insert(identifier.to_owned(), handler);

    Ok(())
  }

  pub async fn run(&mut self, port: u16) -> Result<(), String> {
    let callbacks = self.callbacks.clone();
    let handle = warp::path::full()
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
      })
      .boxed();

    let (sender, reciever) = oneshot::channel::<()>();
    self.shutdown_signal = Some(sender);

    let (_, server) =
      warp::serve(handle).bind_with_graceful_shutdown(([127, 0, 0, 1], port), async {
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
