mod app_message;

/// Runtime traits behind the `strum_macros` derives emitted by `foxtive-macros`
/// (`EnumMessage`, `IntoEnumIterator`, `VariantArray`, ...).
#[cfg(feature = "strum")]
pub use strum;

pub use app_message::AppMessage;
