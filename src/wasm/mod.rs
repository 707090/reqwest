use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

pub use self::body::Body;
pub use self::client::{Client, ClientBuilder};
pub use self::request::{Request, RequestBuilder};
pub use self::response::Response;

mod body;
mod client;
mod request;
mod response;
/// TODO
pub mod multipart;

async fn promise<T>(promise: js_sys::Promise) -> Result<T, crate::error::BoxError>
    where
        T: JsCast,
{
    let js_val = JsFuture::from(promise)
        .await
        .map_err(crate::error::wasm)?;

    js_val
        .dyn_into::<T>()
        .map_err(|_js_val| {
            "promise resolved to unexpected type".into()
        })
}

pub(crate) mod timeout {
    use std::convert::TryFrom;
    use std::time::Duration;

    use wasm_bindgen::{closure::Closure, JsCast, UnwrapThrowExt};
    use web_sys::{
        AbortController,
        AbortSignal,
        window,
    };

    pub struct FetchTimeout {
        timeout_id: i32,
        _abort_handler: Closure<dyn FnMut()>,
        signal: AbortSignal,
    }

    impl FetchTimeout {
        pub fn new(timeout: Duration) -> FetchTimeout {
            let controller = AbortController::new().expect_throw("Creating AbortController cannot fail");
            let signal = controller.signal();
            let _abort_handler = Closure::wrap(Box::new(move || {
                controller.abort();
            }) as Box<dyn FnMut()>);

            let timeout_ms = i32::try_from(timeout.as_millis()).expect_throw("Timeout too large");
            let timeout_id = window()
                .expect_throw("Fetch API requires window")
                .set_timeout_with_callback_and_timeout_and_arguments_0(_abort_handler.as_ref().unchecked_ref(), timeout_ms)
                .expect_throw("Failed to create request timeout");

            FetchTimeout {
                timeout_id,
                _abort_handler,
                signal,
            }
        }

        pub fn signal(&self) -> &AbortSignal {
            &self.signal
        }
    }

    impl Drop for FetchTimeout {
        fn drop(&mut self) {
            window()
                .expect_throw("Fetch API requires window")
                .clear_timeout_with_handle(self.timeout_id);
        }
    }
}