use std::str::FromStr;
use std::sync::{Arc, Mutex, RwLock};

use riker::actors::*;

pub use opcua_client::prelude::*;

pub use crate::app::AppMessage;

struct Connection {
    client: Client,
    session: Option<Arc<RwLock<Session>>>,
}

impl Default for Connection {
    fn default() -> Self {
        let client = ClientBuilder::default()
            .application_name("OPCUA Explorer")
            .application_uri("urn:OPCUAExplorer")
            .product_uri("urn:OPCUAExplorer")
            .trust_server_certs(true)
            .create_sample_keypair(true)
            .session_retry_limit(3)
            .client()
            .unwrap();

        Self {
            client,
            session: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ModelMessage {
    Connect(String, SecurityPolicy, MessageSecurityMode),
}

pub struct Model {
    connection: Arc<Mutex<Connection>>,
    app: ActorRef<AppMessage>,
}

impl ActorFactoryArgs<ActorRef<AppMessage>> for Model {
    fn create_args(app: ActorRef<AppMessage>) -> Self {
        Self {
            connection: Arc::new(Mutex::new(Connection::default())),
            app,
        }
    }
}

impl Actor for Model {
    type Msg = ModelMessage;

    fn recv(&mut self, _ctx: &Context<ModelMessage>, msg: ModelMessage, _sender: Sender) {
        match msg {
            ModelMessage::Connect(endpoint_url, security_policy, message_security_mode) => {
                self.connect(&endpoint_url, security_policy, message_security_mode);
            }
        }
    }
}

impl Model {
    pub fn log<T>(&self, msg: T)
    where
        T: Into<String>,
    {
        self.app.tell(AppMessage::Console(msg.into()), None);
    }

    pub fn connect(
        &self,
        endpoint_url: &str,
        security_policy: SecurityPolicy,
        message_security_mode: MessageSecurityMode,
    ) {
        let user_token_policy = UserTokenPolicy::anonymous();
        let identity_token = IdentityToken::Anonymous;

        self.log(format!(
            "Attempting to connection to endpoint \"{}\"",
            endpoint_url
        ));

        let mut connection = self.connection.lock().unwrap();
        match connection.client.connect_to_endpoint(
            (
                endpoint_url,
                security_policy.to_str(),
                message_security_mode,
                user_token_policy,
            ),
            identity_token,
        ) {
            Ok(session) => {
                self.log("Connection succeeded");
                connection.session = Some(session);
            }
            Err(err) => {
                self.log(format!("Connection failed, status code = {}", err));
                connection.session = None;
            }
        }
    }

    pub fn subscribe_to_items(&self, node_ids: &[String]) {
        let connection = self.connection.lock().unwrap();
        if let Some(ref session) = connection.session {
            let callback = DataChangeCallback::new(|_| {
                // TODO datachange
                println!("datachange");
            });

            // Create a subscription
            let mut session = session.try_write().unwrap();
            match session.create_subscription(1.0, 100, 100, 100, 0, true, callback) {
                Ok(subscription_id) => {
                    let items_to_create = node_ids
                        .iter()
                        .map(|n| NodeId::from_str(n.as_ref()).unwrap())
                        .map(|n| MonitoredItemCreateRequest {
                            item_to_monitor: n.into(),
                            monitoring_mode: MonitoringMode::Reporting,
                            requested_parameters: MonitoringParameters::default(),
                        })
                        .collect::<Vec<MonitoredItemCreateRequest>>();

                    // Create monitored items on the subscription
                    let _ = session.create_monitored_items(
                        subscription_id,
                        TimestampsToReturn::Both,
                        &items_to_create,
                    );
                }
                Err(err) => {
                    println!("Cannot create subscription, error = {}", err);
                }
            }
        }
    }
}
