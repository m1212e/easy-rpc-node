use napi::{Env, JsObject, JsUnknown};

use crate::erpc::server::Socket;

#[napi(object)]
pub struct TargetOptions {
  pub port: u16,
  pub address: String,
}

#[napi(js_name = "ERPCTarget")]
pub struct ERPCTarget {
  target: crate::erpc::target::ERPCTarget,
}

#[napi]
impl ERPCTarget {
  #[napi(constructor)]
  pub fn new(options: TargetOptions, target_type: String) -> Self {
    ERPCTarget {
      target: crate::erpc::target::ERPCTarget::new(
        options.address,
        options.port,
        target_type,
      ),
    }
  }

  #[napi(skip_typescript)]
  pub fn call(
    &self,
    env: Env,
    method_identifier: String,
    parameters: JsUnknown,
  ) -> Result<JsObject, napi::Error> {
    let params: Vec<serde_json::Value> = env.from_js_value(parameters)?;
    let t = self.target.clone();

    env.execute_tokio_future(
      async move {
        let res: serde_json::Value = match t.call(method_identifier, params).await {
          Ok(v) => v,
          Err(err) => {
            return Err(napi::Error::from_reason(err));
          }
        };

        Ok(res)
      },
      |env, data| {
        let ret: JsUnknown = env.to_js_value(&data)?;
        Ok(ret)
      },
    )
  }

  #[napi(skip_typescript, js_name = "setERPCSocket")]
  pub fn set_erpc_socket(&self, env: Env, socket: JsObject) -> Result<(), napi::Error> {
    let mut t = self.target.clone();
    let socket: &mut Socket = env.unwrap(&socket)?;
    let socket = socket.clone();
    env.execute_tokio_future(
      async move {
        t.listen_on_socket(socket).await;
        Ok(())
      },
      |_, _| Ok(()),
    )?;
    Ok(())
  }
}
