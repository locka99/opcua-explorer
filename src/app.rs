use std::{
    collections::HashMap,
    rc::Rc,
    sync::{mpsc, Arc, Mutex, RwLock},
};

use glib::clone;
use gtk::{self, prelude::*};

use riker::actors::*;

use opcua_client::prelude::*;

use crate::model::{Model, ModelMessage};
use crate::new_connection_dlg::NewConnectionDlg;

#[derive(Debug, Clone)]
pub enum AppMessage {
    Console(String),
    Quit,
    Connected,
    Disconnected,
    BrowseNodeResult(NodeId, BrowseResult),
}

#[derive(Debug, Clone)]
struct AppActor {
    tx: Arc<Mutex<mpsc::Sender<AppMessage>>>,
}

impl Actor for AppActor {
    type Msg = AppMessage;

    fn recv(&mut self, _ctx: &Context<AppMessage>, msg: AppMessage, _sender: Sender) {
        let tx = self.tx.lock().unwrap();
        let _ = tx.send(msg);
    }
}

impl ActorFactoryArgs<Arc<Mutex<mpsc::Sender<AppMessage>>>> for AppActor {
    fn create_args(tx: Arc<Mutex<mpsc::Sender<AppMessage>>>) -> Self {
        Self { tx }
    }
}

pub struct App {
    actor_system: ActorSystem,
    builder: Rc<gtk::Builder>,
    model: ActorRef<ModelMessage>,
    toolbar_connect_btn: Rc<gtk::ToolButton>,
    toolbar_disconnect_btn: Rc<gtk::ToolButton>,
    address_space_tree: Rc<gtk::TreeView>,
    console_text_view: Rc<gtk::TextView>,
    address_space_map: HashMap<NodeId, gtk::TreeIter>,
    rx: mpsc::Receiver<AppMessage>,
}

impl App {
    pub fn run() {
        if gtk::init().is_err() {
            println!("Failed to initialize GTK.");
            return;
        }

        // Create some actors to allow stuff to be sent around by messages. Note there is a kludge
        // because GTK is single threaded, so the app actor posts messages via mpsc to a channel
        // that an idle handler will consume.
        let (tx, rx) = mpsc::channel();
        let actor_system = ActorSystem::new().unwrap();
        let app_actor = actor_system
            .actor_of_args::<AppActor, _>("app", Arc::new(Mutex::new(tx)))
            .unwrap();
        let model = actor_system
            .actor_of_args::<Model, _>("model", app_actor)
            .unwrap();

        // The user interface is defined as a .glade file
        let glade_src = include_str!("ui.glade");
        let builder = Rc::new(gtk::Builder::from_string(glade_src));

        let toolbar_connect_btn: Rc<gtk::ToolButton> =
            Rc::new(builder.get_object("toolbar_connect_btn").unwrap());

        let toolbar_disconnect_btn: Rc<gtk::ToolButton> =
            Rc::new(builder.get_object("toolbar_disconnect_btn").unwrap());

        // Address space explorer pane
        let address_space_tree: Rc<gtk::TreeView> =
            Rc::new(builder.get_object("address_space_tree").unwrap());

        // Log / console window
        let console_text_view: Rc<gtk::TextView> =
            Rc::new(builder.get_object("console_text_view").unwrap());

        // Main window
        let main_window: Rc<gtk::ApplicationWindow> =
            Rc::new(builder.get_object("main_window").unwrap());

        let app_rc = Arc::new(RwLock::new(App {
            actor_system,
            builder: builder.clone(),
            console_text_view,
            toolbar_connect_btn,
            toolbar_disconnect_btn,
            address_space_tree,
            address_space_map: HashMap::new(),
            model: model.clone(),
            rx,
        }));

        // Hook up the toolbar buttons
        {
            let app = app_rc.read().unwrap();

            let model_connect = model.clone();
            app.toolbar_connect_btn.connect_clicked(
                clone!(@weak app_rc, @weak builder => move |_| {
                    // Modally show the connect dialog
                    let dlg = NewConnectionDlg::new(model_connect.clone(), builder);
                    dlg.show();
                }),
            );

            let model_disconnect = model.clone();
            app.toolbar_disconnect_btn
                .connect_clicked(clone!(@weak app_rc => move |_| {
                    let app = app_rc.read().unwrap();
                    model_disconnect.tell(ModelMessage::Disconnect, None);
                }));

            // Address space
            app.address_space_tree.connect_expand_collapse_cursor_row(
                clone!(@weak app_rc => @default-return false, move |_, logical, expand, open_all| {
                    let app = app_rc.read().unwrap();
                    app.on_expand_collapse_cursor_row(logical, expand, open_all);
                    true
                }),
            );

            // Monitored item pane
            // TODO

            // Monitored item properties pane
            // TODO

            main_window.connect_delete_event(|_, _| {
                println!("Application is closing");
                gtk::main_quit();
                Inhibit(false)
            });

            app.update_connection_state(false);
            app.console_write("Click Connect... to connect to an OPC UA end point");
        }

        glib::idle_add_local(move || {
            let mut app = app_rc.write().unwrap();
            let processed_msg = app.handle_messages();
            Continue(processed_msg)
        });

        main_window.show_all();

        gtk::main();
    }

    pub fn handle_messages(&mut self) -> bool {
        if let Ok(msg) = self.rx.try_recv() {
            match msg {
                AppMessage::Console(message) => self.console_write(&message),
                AppMessage::Quit => {
                    return false;
                }
                AppMessage::Connected => self.on_connected(),
                AppMessage::Disconnected => self.on_disconnected(),
                AppMessage::BrowseNodeResult(parent_node_id, browse_result) => {
                    self.on_browse_node_result(parent_node_id, browse_result)
                }
            }
            true
        } else {
            false
        }
    }

    pub fn console_write(&self, message: &str) {
        let buffer = self.console_text_view.get_buffer().unwrap();
        let mut end_iter = buffer.get_end_iter();
        buffer.insert(&mut end_iter, message);
        buffer.insert(&mut end_iter, "\n");
    }

    pub fn on_expand_collapse_cursor_row(
        &self,
        _logical: bool,
        expand: bool,
        _open_all: bool,
    ) -> bool {
        if expand {
            // Get tree view cursor
            let (tree_path, _) = self.address_space_tree.get_cursor();
            if let Some(tree_path) = tree_path {
                let idx = tree_path.get_indices();
                let address_space_model: gtk::TreeStore =
                    self.builder.get_object("address_space_model").unwrap();
            }
        }
        false
    }

    pub fn on_connected(&self) {
        self.update_connection_state(true);
        self.populate_address_space();
    }

    pub fn on_disconnected(&self) {
        self.update_connection_state(false);
    }

    pub fn on_browse_node_result(
        &mut self,
        parent_node_id: NodeId,
        browse_node_result: BrowseResult,
    ) {
        // TODO get the parent node in the tree
        // TODO clear any existing children

        if browse_node_result.status_code.is_good() {
            let address_space_model: gtk::TreeStore =
                self.builder.get_object("address_space_model").unwrap();

            let parent = if parent_node_id == ObjectId::RootFolder.into() {
                None
            } else {
                // TODO find parent node in the tree
                None
            };

            // This code only works for root node and needs to be fixed
            if let Some(references) = browse_node_result.references {
                references.iter().for_each(|r| {
                    println!("Result = {:?}", r);
                    let node_id = format!("{}", r.node_id);
                    let browse_name = format!("{}", r.browse_name.name);
                    let display_name = format!("{}", r.display_name);
                    let t = "i=333"; //TODO
                    let values: Vec<&dyn ToValue> = vec![&node_id, &browse_name, &display_name, &t];

                    let i = address_space_model.insert_with_values(
                        parent,
                        None,
                        &[0, 1, 2, 3],
                        &values,
                    );

                    self.address_space_map.insert(r.node_id.node_id.clone(), i);
                });
            }
        }
    }

    pub fn clear_address_space(&self) {
        let address_space_model: gtk::TreeStore =
            self.builder.get_object("address_space_model").unwrap();
        address_space_model.clear();
    }

    pub fn populate_address_space(&self) {
        self.clear_address_space();
        self.model
            .send_msg(ModelMessage::BrowseNode(ObjectId::RootFolder.into()), None);
    }

    pub fn update_connection_state(&self, is_connected: bool) {
        self.toolbar_connect_btn.set_sensitive(!is_connected);
        self.toolbar_disconnect_btn.set_sensitive(is_connected);
    }
}
