/// Wall clock for durable timestamps. Native callers retain SystemTime's
/// before-epoch error behavior; browsers read the host's Unix millisecond clock.
pub(crate) fn system_time_now() -> std::time::SystemTime {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
    }
    #[cfg(target_arch = "wasm32")]
    {
        std::time::UNIX_EPOCH + std::time::Duration::from_millis(js_sys::Date::now() as u64)
    }
}
