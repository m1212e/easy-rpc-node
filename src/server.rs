#![deny(clippy::all)]

//TODO: remove unwraps
//TODO: refactoring

use std::{thread, time::Duration};

use napi::{
  bindgen_prelude::{FromNapiValue, Promise},
  Env, JsFunction, JsUnknown, NapiRaw,
};
use tokio::sync::mpsc;

use crate::erpc::server::Socket;

#[napi(object)]
pub struct ServerOptions {
  pub port: u16,
  pub allowed_cors_origins: Vec<String>,
}

#[napi(js_name = "ERPCServer")]
pub struct ERPCServer {
  server: crate::erpc::server::ERPCServer,
}

#[napi]
impl ERPCServer {
  #[napi(constructor)]
  pub fn new(
    options: ServerOptions,
    server_type: String,
    enable_sockets: bool,
    role: String,
  ) -> Self {
    ERPCServer {
      server: crate::erpc::server::ERPCServer::new(options.port, options.allowed_cors_origins),
    }
  }

  #[napi(skip_typescript, js_name = "registerERPCCallbackFunction")]
  pub fn register_erpc_callback_function(
    &mut self,
    env: Env,
    func: JsFunction,
    identifier: String,
  ) -> Result<(), napi::Error> {
    let tsf = match crate::threadsafe_function::ThreadsafeFunction::create(
      env.raw(),
      unsafe { func.raw() },
      0,
      |ctx: crate::threadsafe_function::ThreadSafeCallContext<(
        Vec<serde_json::Value>,
        mpsc::Sender<String>,
      )>| {
        let args = ctx
          .value
          .0
          .iter()
          .map(|v| ctx.env.to_js_value(v))
          .collect::<Result<Vec<JsUnknown>, napi::Error>>()?;

        let response = ctx.callback.call(None, &args)?;
        let response_channel = ctx.value.1.clone();

        if !response.is_promise()? {
          // String::from_napi_value(ctx.env.raw(), response.raw())?
          let response: serde_json::Value = ctx.env.from_js_value(response)?;
          ctx
            .env
            .execute_tokio_future(
              async move {
                match response_channel
                  .send(serde_json::to_string(&response)?)
                  .await
                {
                  Ok(_) => {}
                  Err(err) => eprintln!("Could not send on return channel: {err}"),
                };
                Ok(())
              },
              |_, _| Ok(()),
            )
            .unwrap();
        } else {
          unsafe {
            let prm: Promise<serde_json::Value> = Promise::from_napi_value(ctx.env.raw(), response.raw())?;
            let response_channel = response_channel.clone();
            ctx.env.execute_tokio_future(
              async move {
                let result = prm.await?;
                match response_channel.send(serde_json::to_string(&result)?).await {
                  Ok(_) => {}
                  Err(err) => eprintln!("Could not send on return channel: {err}"),
                };
                Ok(())
              },
              |_, _| Ok(()),
            )?;
          }
        };

        Ok(())
      },
    ) {
      Ok(v) => v,
      Err(err) => {
        return Err(napi::Error::from_reason(format!(
          "Could not create threadsafe function for {identifier}: {err}"
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
          false => match serde_json::from_str(&input) {
            Ok(v) => v,
            Err(err) => {
              return Box::pin(
                async move { Err(format!("Could not parse input into Value: {err}")) },
              );
            }
          },
        };

        //TODO change this back to oneshot
        let (sender, mut reciever) = mpsc::channel::<String>(1);
        let r = tsf.call(
          (val, sender),
          crate::threadsafe_function::ThreadsafeFunctionCallMode::Blocking,
        );

        match r {
          napi::Status::Ok => {}
          _ => {
            return Box::pin(async move { Err(format!("Threadsafe function status not ok: {r}")) });
          }
        }

        // thread::sleep(Duration::from_millis(1000));

        Box::pin(async move {
          match reciever.recv().await {
            Some(v) => Ok(v),
            None => {
              panic!("ended");
            }
          }
        })

        // Box::pin(async move { Ok(reciever.recv().await.unwrap()) })
      }),
      &identifier,
    ) {
      Ok(_) => Ok(()),
      Err(err) => return Err(napi::Error::from_reason(err)),
    }
  }

  #[napi(skip_typescript)]
  pub fn on_socket_connection(&mut self, env: Env, func: JsFunction) -> Result<(), napi::Error> {
    let tsf = match crate::threadsafe_function::ThreadsafeFunction::create(
      env.raw(),
      unsafe { func.raw() },
      0,
      |ctx: crate::threadsafe_function::ThreadSafeCallContext<Socket>| {
        let role = ctx.env.create_string_from_std(ctx.value.role.clone())?;
        let mut socket = ctx.env.create_object()?;
        ctx.env.wrap(&mut socket, ctx.value)?;

        ctx
          .callback
          .call(None, &vec![role.into_unknown(), socket.into_unknown()])?;
        Ok(())
      },
    ) {
      Ok(v) => v,
      Err(err) => {
        return Err(napi::Error::from_reason(format!(
          "Could not create threadsafe function for socket callback: {err}"
        )))
      }
    };

    self
      .server
      .register_socket_connection_callback(Box::new(move |socket| {
        let r = tsf.call(
          (*socket).to_owned(),
          crate::threadsafe_function::ThreadsafeFunctionCallMode::Blocking,
        );

        match r {
          napi::Status::Ok => {}
          _ => {
            return eprintln!("Threadsafe function status not ok: {r}");
          }
        };
      }))
      .unwrap();
    Ok(())
  }

  /**
   * Starts the server as configured
   */
  #[napi]
  pub async fn run(&mut self) -> Result<(), napi::Error> {
    match self.server.run().await {
      Ok(_) => Ok(()),
      Err(err) => Err(napi::Error::from_reason(err)),
    }
  }

  /**
   * Stops the server
   */
  #[napi]
  pub fn stop(&mut self) -> Result<(), napi::Error> {
    match self.server.stop() {
      Ok(_) => Ok(()),
      Err(err) => Err(napi::Error::from_reason(err)),
    }
  }
}
