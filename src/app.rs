use std::{
    collections::HashMap,
    rc::Rc,
    sync::{mpsc, Arc, Mutex, RwLock},
};

use glib::clone;
use gtk::{self, prelude::*, TreeIter, TreePath};

use riker::actors::*;

use opcua_client::prelude::*;

use crate::model::{Model, ModelMessage};
use crate::new_connection_dlg::NewConnectionDlg;
use std::str::FromStr;

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
    rx: mpsc::Receiver<AppMessage>,
    builder: Rc<gtk::Builder>,
    model: ActorRef<ModelMessage>,
    toolbar_connect_btn: Rc<gtk::ToolButton>,
    toolbar_disconnect_btn: Rc<gtk::ToolButton>,
    address_space_tree: Rc<gtk::TreeView>,
    console_text_view: Rc<gtk::TextView>,
    address_space_map: HashMap<NodeId, gtk::TreeIter>,
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
            .actor_of_args::<Model, _>("model", app_actor.clone())
            .unwrap();

        // The user interface is defined as a .glade file
        let glade_src = include_str!("ui.glade");
        let builder = Rc::new(gtk::Builder::from_string(glade_src));

        // Main window
        let main_window: Rc<gtk::ApplicationWindow> =
            Rc::new(builder.get_object("main_window").unwrap());

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

        let app = Arc::new(RwLock::new(App {
            actor_system,
            rx,
            builder: builder.clone(),
            console_text_view: console_text_view.clone(),
            toolbar_connect_btn: toolbar_connect_btn.clone(),
            toolbar_disconnect_btn: toolbar_disconnect_btn.clone(),
            address_space_tree: address_space_tree.clone(),
            address_space_map: HashMap::new(),
            model: model.clone(),
        }));

        // Hook up the toolbar buttons

        let model_connect = model.clone();
        let _id =
            toolbar_connect_btn.connect_clicked(clone!(@weak app, @weak builder => move |_| {
                println!("toolbar_connect_btn click");
                // Show the connect dialog
                let dlg = NewConnectionDlg::new(model_connect.clone(), builder);
                dlg.show();
            }));

        let model_disconnect = model.clone();
        let _id = toolbar_disconnect_btn.connect_clicked(clone!(@weak app => move |_| {
            println!("toolbar_disconnect_btn click");
            model_disconnect.tell(ModelMessage::Disconnect, None);
        }));

        // Address space
        let _id =
            address_space_tree.connect_row_expanded(clone!(@weak app => move |_, iter, path| {
                let app = app.read().unwrap();
                app.address_view_row_expanded(iter, path);
            }));

        // Monitored item pane
        // TODO

        // Monitored item properties pane
        // TODO
        main_window.connect_delete_event(|_, _| {
            println!("Application is closing");
            gtk::main_quit();
            Inhibit(false)
        });

        {
            let app = app.read().unwrap();
            app.update_connection_state(false);
            app.console_write("Click Connect... to connect to an OPC UA end point");
        }

        glib::idle_add_local(move || {
            let mut app = app.write().unwrap();
            let quit = !app.handle_messages();
            Continue(!quit)
        });

        main_window.show_all();

        // Main loop
        gtk::main();

        println!("Finished");
    }

    pub fn handle_messages(&mut self) -> bool {
        if let Ok(msg) = self.rx.try_recv() {
            println!("try_recv msg = #{:?}", msg);
            match msg {
                AppMessage::Console(message) => self.console_write(&message),
                AppMessage::Connected => self.on_connected(),
                AppMessage::Disconnected => self.on_disconnected(),
                AppMessage::BrowseNodeResult(parent_node_id, browse_result) => {
                    self.on_browse_node_result(parent_node_id, browse_result)
                }
                AppMessage::Quit => {
                    println!("Application was told to quit");
                    return false;
                }
            }
        }
        true
    }

    pub fn console_write(&self, message: &str) {
        let buffer = self.console_text_view.get_buffer().unwrap();
        let mut end_iter = buffer.get_end_iter();
        buffer.insert(&mut end_iter, message);
        buffer.insert(&mut end_iter, "\n");
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
        println!("browse node result");

        // TODO get the parent node in the tree
        // TODO clear any existing children

        if browse_node_result.status_code.is_good() {
            let address_space_model: gtk::TreeStore =
                self.builder.get_object("address_space_model").unwrap();

            let parent = if parent_node_id == ObjectId::RootFolder.into() {
                None
            } else if let Some(iter) = self.address_space_map.get(&parent_node_id) {
                if !Self::remove_address_space_dummy_node(&address_space_model, iter) {
                    println!(
                        "Parent node doesn't have a dummy node, so maybe this is a race condition"
                    );
                    return;
                }
                Some(iter.clone())
            } else {
                println!(
                    "Parent node id {:?} doesn't exist so browse will do nothing",
                    parent_node_id
                );
                return;
            };

            // This code only works for root node and needs to be fixed
            if let Some(references) = browse_node_result.references {
                references.iter().for_each(|r| {
                    self.insert_address_space_reference(&address_space_model, r, parent.clone());
                });
            }
        }
    }

    const COL_DUMMY: u32 = 0;
    const COL_NODE_ID: u32 = 1;
    const COL_BROWSE_NAME: u32 = 2;
    const COL_DISPLAY_NAME: u32 = 3;
    const COL_REFERENCE_TYPE_ID: u32 = 4;

    pub fn address_view_row_expanded(&self, iter: &TreeIter, path: &TreePath) -> bool {
        println!("address_view_row_expanded");

        let address_space_model: gtk::TreeStore =
            self.builder.get_object("address_space_model").unwrap();

        if Self::has_address_space_dummy_node(&address_space_model, iter) {
            let v = address_space_model.get_value(iter, Self::COL_NODE_ID as i32);
            if let Ok(Some(node_id)) = v.get::<String>() {
                println!("Getting nodes organized by {:?}", node_id);
                let node_id = NodeId::from_str(&node_id).unwrap();
                // Initiate a browse on the node
                self.model.tell(ModelMessage::BrowseNode(node_id), None)
            } else {
                println!("Cannot get node id from iterator {:?}", iter);
                println!("Node id Value = {:?}", v);
            }
        }
        false
    }

    fn insert_address_space_reference(
        &mut self,
        address_space_model: &gtk::TreeStore,
        r: &ReferenceDescription,
        parent: Option<TreeIter>,
    ) {
        println!("Result = {:?}", r);
        let dummy_node = false;
        let node_id = format!("{}", r.node_id.node_id);
        let browse_name = format!("{}", r.browse_name.name);
        let display_name = format!("{}", r.display_name);
        let reference_type_id = format!("{}", r.reference_type_id);

        let columns = &[
            Self::COL_DUMMY,
            Self::COL_NODE_ID,
            Self::COL_BROWSE_NAME,
            Self::COL_DISPLAY_NAME,
            Self::COL_REFERENCE_TYPE_ID,
        ];
        let values: Vec<&dyn ToValue> = vec![
            &dummy_node,
            &node_id,
            &browse_name,
            &display_name,
            &reference_type_id,
        ];

        // Insert element into tree
        let i = if let Some(parent) = parent.clone() {
            let parent = Some(&parent);
            Self::insert_with_values(address_space_model, parent, None, columns, &values)
        } else {
            Self::insert_with_values(address_space_model, None, None, columns, &values)
        };

        // Insert a dummy node under the reference
        Self::insert_address_space_dummy_node(address_space_model, i.clone());

        println!("Adding mapping between {:?} and {:?}", r.node_id.node_id, i);

        self.address_space_map
            .insert(r.node_id.node_id.clone(), i.clone());
    }

    fn insert_address_space_dummy_node(
        address_space_model: &gtk::TreeStore,
        parent: TreeIter,
    ) -> TreeIter {
        let dummy_node = true;
        let node_id = "";
        let browse_name = "";
        let display_name = "";
        let reference_type_id = "";
        let columns = &[
            Self::COL_DUMMY,
            Self::COL_NODE_ID,
            Self::COL_BROWSE_NAME,
            Self::COL_DISPLAY_NAME,
            Self::COL_REFERENCE_TYPE_ID,
        ];
        let values: Vec<&dyn ToValue> = vec![
            &dummy_node,
            &node_id,
            &browse_name,
            &display_name,
            &reference_type_id,
        ];

        Self::insert_with_values(address_space_model, Some(&parent), None, columns, &values)
    }

    fn has_address_space_dummy_node(
        address_space_model: &gtk::TreeStore,
        parent: &TreeIter,
    ) -> bool {
        if let Some(child_iter) = address_space_model.iter_children(Some(parent)) {
            let v = address_space_model.get_value(&child_iter, Self::COL_DUMMY as i32);
            let mut next_iter = true;
            while next_iter {
                if let Ok(Some(is_dummy)) = v.get::<bool>() {
                    if is_dummy {
                        return true;
                    }
                }
                next_iter = address_space_model.iter_next(&child_iter);
            }
        }
        false
    }

    fn remove_address_space_dummy_node(
        address_space_model: &gtk::TreeStore,
        parent: &TreeIter,
    ) -> bool {
        if let Some(child_iter) = address_space_model.iter_children(Some(parent)) {
            let v = address_space_model.get_value(&child_iter, Self::COL_DUMMY as i32);
            let mut next_iter = true;
            while next_iter {
                if let Ok(Some(is_dummy)) = v.get::<bool>() {
                    if is_dummy {
                        // Remove the element
                        address_space_model.remove(&child_iter);
                        return true;
                    }
                }
                next_iter = address_space_model.iter_next(&child_iter);
            }
        }

        false
    }

    fn insert_with_values(
        address_space_model: &gtk::TreeStore,
        parent: Option<&TreeIter>,
        position: Option<u32>,
        columns: &[u32],
        values: &[&dyn ToValue],
    ) -> TreeIter {
        address_space_model.insert_with_values(parent, position, columns, &values)
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
