// inspired by https://github.com/actix/actix-web/blob/web-v3.3.2/src/handler.rs

use erased_serde::Serialize;
use futures_util::Future;
use serde::de::DeserializeOwned;

pub trait Handler<Parameters: DeserializeOwned>: Send + Sync + Clone {
  type Output: Serialize;
  type Future: Future<Output = Result<Self::Output, String>> + Send + Sync;

  fn run(&self, parameters: Parameters) -> Self::Future;
  fn deserialize_parameters(
    &self,
    raw_parameters: serde_json::Value,
  ) -> Result<Parameters, serde_json::error::Error>;
}

macro_rules! factory ({ $($param:ident)* } => {
    impl<Func, Fut, Out, $($param,)*> Handler<($($param,)*)> for Func
    where
        Func: Fn($($param),*) -> Fut + Send + Sync + Clone,
        Fut: Future<Output = Result<Out, String>> + Send + Sync,
        ($($param,)*): DeserializeOwned,
        Out: Serialize,
    {
        type Output = Out;
        type Future = Fut;

        fn run(&self, ($($param,)*): ($($param,)*)) -> Self::Future {
            (self)($($param,)*)
        }

        fn deserialize_parameters(&self, raw_parameters: serde_json::Value) -> Result<($($param,)*), serde_json::error::Error> {
            serde_json::from_value(raw_parameters)
        }
    }
});

factory! {}
factory! { A }
factory! { A B }
factory! { A B C }
factory! { A B C D }
factory! { A B C D E }
factory! { A B C D E F }
factory! { A B C D E F G }
factory! { A B C D E F G H }
factory! { A B C D E F G H I }
factory! { A B C D E F G H I J }
factory! { A B C D E F G H I J K }
factory! { A B C D E F G H I J K L }
factory! { A B C D E F G H I J K L M }
factory! { A B C D E F G H I J K L M N }
factory! { A B C D E F G H I J K L M N O }
factory! { A B C D E F G H I J K L M N O P }
factory! { A B C D E F G H I J K L M N O P Q }
factory! { A B C D E F G H I J K L M N O P Q R }
factory! { A B C D E F G H I J K L M N O P Q R S }
factory! { A B C D E F G H I J K L M N O P Q R S T}
