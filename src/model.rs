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
    Disconnect,
    BrowseNode(NodeId),
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
                self.connect(&endpoint_url, security_policy, message_security_mode)
            }
            ModelMessage::Disconnect => self.disconnect(),
            ModelMessage::BrowseNode(parent_node_id) => self.browse_node(parent_node_id),
        }
    }
}

impl Model {
    pub fn log<T>(&self, msg: T)
    where
        T: Into<String>,
    {
        self.send_app_msg(AppMessage::Console(msg.into()));
    }

    fn send_app_msg(&self, message: AppMessage) {
        self.app.tell(message, None);
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
                {
                    let mut session = session.write().unwrap();
                    session.set_connection_status_callback(ConnectionStatusCallback::new(
                        |connected| {
                            println!("Connection status change TODO");
                        },
                    ));
                }

                self.log("Connection succeeded");
                self.send_app_msg(AppMessage::Connected);
                connection.session = Some(session);
            }
            Err(err) => {
                self.log(format!("Connection failed, status code = {}", err));
                connection.session = None;
                self.send_app_msg(AppMessage::Disconnected);
            }
        }
    }

    pub fn disconnect(&self) {
        let mut connection = self.connection.lock().unwrap();
        if let Some(ref session) = connection.session {
            let mut session = session.write().unwrap();
            session.disconnect();
            self.log("Disconnecting from session");
        }
        connection.session = None;
        self.send_app_msg(AppMessage::Disconnected);
    }

    pub fn browse_node(&self, parent_node_id: NodeId) {
        let connection = self.connection.lock().unwrap();
        if let Some(ref session) = connection.session {
            self.log(format!("Fetching children of node {}", parent_node_id));

            let mut session = session.write().unwrap();
            let browse_description = BrowseDescription {
                node_id: parent_node_id.clone(),
                browse_direction: BrowseDirection::Forward,
                reference_type_id: ReferenceTypeId::Organizes.into(),
                include_subtypes: true,
                node_class_mask: 0xff,
                result_mask: 0xff,
            };
            if let Ok(results) = session.browse(&[browse_description]) {
                if let Some(mut results) = results {
                    self.send_app_msg(AppMessage::BrowseNodeResult(
                        parent_node_id,
                        results.remove(0),
                    ));
                } else {
                    self.log("Fetch failed, no results");
                }
            } else {
                self.log("Fetch failed, no results")
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
