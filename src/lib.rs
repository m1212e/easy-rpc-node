#![deny(clippy::all)]

use std::sync::Arc;

use napi::{
  bindgen_prelude::Promise, Env, Error, JsFunction, JsObject, JsString, JsUnknown, NapiRaw,
};
use serde_json::json;
use tokio::sync::oneshot;

mod server;
mod threadsafe_function;

#[macro_use]
extern crate napi_derive;

#[napi(object)]
pub struct ServerOptions {
  pub port: u16,
  pub allowed_cors_origins: Vec<String>,
}

#[napi]
pub struct ERPCServer {
  options: ServerOptions,
  server: server::ERPCServer,
}

#[napi]
impl ERPCServer {
  #[napi(constructor)]
  pub fn new(
    options: ServerOptions,
    types: Vec<String>,
    enable_sockets: bool,
    role: String,
  ) -> Self {
    ERPCServer {
      options,
      server: server::ERPCServer::new(),
    }
  }

  #[napi(skip_typescript, js_name = "registerERPCCallbackFunction")]
  pub fn register_erpc_callback_function(
    &mut self,
    env: Env,
    func: JsFunction,
    identifier: String,
  ) -> Result<(), napi::Error> {
    let tsf = match threadsafe_function::ThreadsafeFunction::create(
      env.raw(),
      unsafe { func.raw() },
      0,
      |ctx: threadsafe_function::ThreadSafeCallContext<(
        Vec<serde_json::Value>,
        oneshot::Sender<Result<String, String>>,
      )>| {
        let args = ctx
          .value
          .0
          .iter()
          .map(|v| ctx.env.to_js_value(v))
          .collect::<Result<Vec<JsUnknown>, napi::Error>>()?;

        let response = ctx.callback.call(None, &args);

        let result = ctx.value.1.send(match response {
          Ok(v) => {
            let v: Result<serde_json::Value, Error> = ctx.env.from_js_value(v);
            match v {
              Ok(v) => match serde_json::to_string(&v) {
                Ok(v) => Ok(v),
                Err(err) => Err(format!("Serialization error(2): {err}")),
              },
              Err(err) => Err(format!("Serialization error(1): {err}")),
            }
          }
          Err(err) => Err(format!("{:#?}", err)),
        });

        match result {
          Ok(_) => {}
          Err(_) => eprintln!("Could not send on oneshot"),
        }
        Ok(())
      },
    ) {
      Ok(v) => v,
      Err(err) => {
        return Err(napi::Error::from_reason(format!(
          "Could not create threadsafe function: {err}"
        )))
      }
    };

    match self.server.register_handler(
      Box::new(move |input| {
        let val: Vec<serde_json::Value> = match input.len() == 0 {
          true => {
            println!("Request body is empty. Defaulting to [] parameters.");
            vec![]
          }
          false => match serde_json::from_slice(&input) {
            Ok(v) => v,
            Err(err) => {
              return Box::pin(
                async move { Err(format!("Could not parse input into Value: {err}")) },
              );
            }
          },
        };

        let (sender, reciever) = oneshot::channel::<Result<String, String>>();
        let r = tsf.call(
          (val, sender),
          threadsafe_function::ThreadsafeFunctionCallMode::Blocking,
        );

        Box::pin(async {
          match reciever.await {
            Ok(v) => match v {
              Ok(v) => Ok(v),
              Err(err) => Err(format!("Handler returned error: {err}")),
            },
            Err(err) => Err(format!("Recv Error in handler result oneshot: {err}")),
          }
        })
      }),
      &identifier,
    ) {
      Ok(_) => Ok(()),
      Err(err) => return Err(napi::Error::from_reason(err)),
    }
  }

  #[napi]
  pub async fn run(&mut self) -> Result<(), napi::Error> {
    match self.server.run(self.options.port).await {
      Ok(_) => Ok(()),
      Err(err) => Err(napi::Error::from_reason(err)),
    }
  }

  #[napi]
  pub fn stop(&mut self) -> Result<(), napi::Error> {
    match self.server.stop() {
      Ok(_) => Ok(()),
      Err(err) => Err(napi::Error::from_reason(err)),
    }
  }
}
