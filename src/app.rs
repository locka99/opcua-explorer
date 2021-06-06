use std::{
    rc::Rc,
    sync::{mpsc, Arc, Mutex, RwLock},
};

use glib::clone;
use gtk::{self, prelude::*};

use riker::actors::*;

use opcua_client::prelude::*;

use crate::{
    address_space_tree_view::*,
    model::{Model, ModelMessage},
    new_connection_dlg::NewConnectionDlg,
};

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
    rx: mpsc::Receiver<AppMessage>,
    toolbar_connect_btn: Rc<gtk::ToolButton>,
    toolbar_disconnect_btn: Rc<gtk::ToolButton>,
    address_space_tree: AddressSpaceTreeView,
    console_text_view: Rc<gtk::TextView>,
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
            rx,
            console_text_view: console_text_view.clone(),
            toolbar_connect_btn: toolbar_connect_btn.clone(),
            toolbar_disconnect_btn: toolbar_disconnect_btn.clone(),
            address_space_tree: AddressSpaceTreeView::new(builder.clone(), model.clone()),
        }));

        // Hook up the toolbar buttons

        let _id =
            toolbar_connect_btn.connect_clicked(clone!(@strong model, @weak builder => move |_| {
                println!("toolbar_connect_btn click");
                // Show the connect dialog
                let dlg = NewConnectionDlg::new(model.clone(), builder);
                dlg.show();
            }));

        let _id = toolbar_disconnect_btn.connect_clicked(clone!(@strong model => move |_| {
            println!("toolbar_disconnect_btn click");
            model.tell(ModelMessage::Disconnect, None);
        }));

        // Address space
        let _id =
            address_space_tree.connect_row_expanded(clone!(@weak app => move |_, iter, path| {
                let app = app.read().unwrap();
                app.address_space_tree.row_expanded(iter, path);
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
        self.address_space_tree.populate();
    }

    pub fn on_disconnected(&self) {
        self.update_connection_state(false);
    }

    pub fn on_browse_node_result(
        &mut self,
        parent_node_id: NodeId,
        browse_node_result: BrowseResult,
    ) {
        self.address_space_tree
            .on_browse_node_result(parent_node_id, browse_node_result);
    }

    pub fn update_connection_state(&self, is_connected: bool) {
        self.toolbar_connect_btn.set_sensitive(!is_connected);
        self.toolbar_disconnect_btn.set_sensitive(is_connected);
    }
}
