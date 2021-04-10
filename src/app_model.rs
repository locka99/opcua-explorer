pub use opcua_client::prelude::*;

pub struct AppModel {}

impl AppModel {
    pub fn new() -> AppModel {
        AppModel {}
    }

    pub fn connect(
        &self,
        host: &str,
        security_policy: SecurityPolicy,
        message_security_mode: MessageSecurityMode,
    ) {
    }
}
