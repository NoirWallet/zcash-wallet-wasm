//! Caller-local monotonic helper timer with cancellation-safe browser handles.
use futures_util::future::LocalBoxFuture;
use std::time::Duration;

pub(crate) struct HelperTimer {
    #[cfg(not(target_arch = "wasm32"))]
    origin: tokio::time::Instant,
}

impl Default for HelperTimer {
    fn default() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            origin: tokio::time::Instant::now(),
        }
    }
}

impl zcash_voting::helper::client::local::Timer for HelperTimer {
    fn now(&self) -> Duration {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.origin.elapsed()
        }
        #[cfg(target_arch = "wasm32")]
        {
            Duration::from_secs_f64(
                web_sys::window()
                    .and_then(|window| window.performance())
                    .map(|clock| clock.now() / 1000.0)
                    .unwrap_or(0.0),
            )
        }
    }

    fn sleep(&self, delay: Duration) -> LocalBoxFuture<'_, ()> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Box::pin(tokio::time::sleep(delay))
        }
        #[cfg(target_arch = "wasm32")]
        {
            Box::pin(async move {
                use wasm_bindgen::{JsCast, closure::Closure};
                struct Timeout {
                    window: web_sys::Window,
                    id: i32,
                    _callback: Closure<dyn FnMut()>,
                }
                impl Drop for Timeout {
                    fn drop(&mut self) {
                        self.window.clear_timeout_with_handle(self.id);
                    }
                }
                let Some(window) = web_sys::window() else {
                    return;
                };
                let (sender, receiver) = futures_channel::oneshot::channel();
                let mut sender = Some(sender);
                let callback = Closure::<dyn FnMut()>::new(move || {
                    if let Some(sender) = sender.take() {
                        let _ = sender.send(());
                    }
                });
                let Ok(id) = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                    callback.as_ref().unchecked_ref(),
                    delay.as_millis().min(i32::MAX as u128) as i32,
                ) else {
                    return;
                };
                let _timeout = Timeout {
                    window,
                    id,
                    _callback: callback,
                };
                let _ = receiver.await;
            })
        }
    }
}
