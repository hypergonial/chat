/// Dispatches an event through the gateway.
///
/// ## Arguments
///
/// * `event` - The GatewayEvent to dispatch.
///
/// ## Example
///
/// ```rust
/// dispatch!(GatewayEvent::MessageCreate(message.clone()));
/// ```

#[macro_export]
macro_rules! dispatch {
    ($event:expr) => {
        APP.write().await.gateway.dispatch($event);
    };
}
