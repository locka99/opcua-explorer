use std::rc::Rc;

use glib::clone;
use gtk::{self, prelude::*};

use riker::actors::*;

use crate::model::{Model, ModelMessage};
use crate::new_connection_dlg::NewConnectionDlg;

#[derive(Debug, Clone)]
pub enum AppMessage {
    Console(String),
}

#[derive(Debug, Clone)]
struct AppActor {}

impl Actor for AppActor {
    type Msg = AppMessage;

    fn recv(&mut self, _ctx: &Context<AppMessage>, msg: AppMessage, _sender: Sender) {
        match msg {
            AppMessage::Console(msg) => {
                println!("Received: {}", msg);
            }
        }
    }
}

impl Default for AppActor {
    fn default() -> Self {
        Self {}
    }
}

pub struct App {
    actor_system: ActorSystem,
    builder: Rc<gtk::Builder>,
    model: ActorRef<ModelMessage>,
    toolbar_connect_btn: Rc<gtk::ToolButton>,
    toolbar_disconnect_btn: Rc<gtk::ToolButton>,
    console_text_view: Rc<gtk::TextView>,
}

impl App {
    pub fn run() {
        if gtk::init().is_err() {
            println!("Failed to initialize GTK.");
            return;
        }

        // The user interface is defined as a .glade file
        let glade_src = include_str!("ui.glade");
        let builder = Rc::new(gtk::Builder::from_string(glade_src));

        let toolbar_connect_btn: Rc<gtk::ToolButton> =
            Rc::new(builder.get_object("toolbar_connect_btn").unwrap());

        let toolbar_disconnect_btn: Rc<gtk::ToolButton> =
            Rc::new(builder.get_object("toolbar_disconnect_btn").unwrap());

        // Log / console window
        let console_text_view: Rc<gtk::TextView> =
            Rc::new(builder.get_object("console_text_view").unwrap());

        let actor_system = ActorSystem::new().unwrap();
        let app_actor = actor_system.actor_of::<AppActor>("app").unwrap();
        let model = actor_system
            .actor_of_args::<Model, _>("model", app_actor)
            .unwrap();

        let app = Rc::new(App {
            actor_system,
            builder,
            toolbar_connect_btn,
            toolbar_disconnect_btn,
            console_text_view,
            model,
        });

        // Hook up the toolbar buttons
        app.toolbar_connect_btn
            .connect_clicked(clone!(@weak app => move |_| {
                app.on_connect_btn_clicked();
            }));

        app.toolbar_disconnect_btn
            .connect_clicked(clone!(@weak app => move |_| {
                app.on_disconnect_btn_clicked();
            }));

        // Address space explorer pane
        // TODO

        // Monitored item pane
        // TODO

        // Monitored item properties pane
        // TODO

        let main_window: gtk::ApplicationWindow = app.builder.get_object("main_window").unwrap();

        main_window.connect_delete_event(|_, _| {
            println!("Application is closing");
            gtk::main_quit();
            Inhibit(false)
        });

        app.update_state();
        app.console_write("Click Connect... to connect to an OPC UA end point");

        main_window.show_all();

        gtk::main();
    }
    pub fn console_write(&self, message: &str) {
        let buffer = self.console_text_view.get_buffer().unwrap();
        let mut end_iter = buffer.get_end_iter();
        buffer.insert(&mut end_iter, message);
        buffer.insert(&mut end_iter, "\n");
    }

    pub fn on_connect_btn_clicked(&self) {
        println!("Clicked!");
        let dlg = NewConnectionDlg::new(self.model.clone(), &self.builder);
        dlg.show();

        // TODO
        self.populate_address_space();
    }

    pub fn on_connect(&self) {}

    pub fn clear_address_space(&self) {
        let address_space_model: gtk::TreeStore =
            self.builder.get_object("address_space_model").unwrap();
        address_space_model.clear();
    }

    pub fn populate_address_space(&self) {
        self.clear_address_space();

        let address_space_model: gtk::TreeStore =
            self.builder.get_object("address_space_model").unwrap();
        for i in 0..10 {
            let v1 = "s=1".to_value();
            let v2 = format!("Browse Name {}", i);
            let v3 = "Display Name".to_value();
            let v4 = "i=333".to_value();
            let values: Vec<&dyn ToValue> = vec![&v1, &v2, &v3, &v4];
            address_space_model.insert_with_values(None, None, &[0, 1, 2, 3], &values);
        }
    }

    pub fn on_disconnect_btn_clicked(&self) {
        println!("Disconnect Clicked!");
    }

    fn is_connected(&self) -> bool {
        false
    }

    pub fn update_state(&self) {
        let is_connected = self.is_connected();
        self.toolbar_connect_btn.set_sensitive(!is_connected);
        self.toolbar_disconnect_btn.set_sensitive(is_connected);
    }
}
