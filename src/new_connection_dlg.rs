use std::rc::Rc;
use std::str::FromStr;

use glib::clone;
use gtk::{self, prelude::*};
use riker::actors::*;

use opcua_client::prelude::*;

use crate::model::ModelMessage;

struct NewConnectionDlgImpl {
    model: ActorRef<ModelMessage>,
    dlg: Rc<gtk::Dialog>,
    security_policy_combo: Rc<gtk::ComboBoxText>,
    message_security_mode_combo: Rc<gtk::ComboBoxText>,
    endpoint_url_text: Rc<gtk::Entry>,
}

pub(crate) struct NewConnectionDlg {
    data: Rc<NewConnectionDlgImpl>,
}

impl NewConnectionDlg {
    pub fn new(model: ActorRef<ModelMessage>, builder: Rc<gtk::Builder>) -> Self {
        let dlg: Rc<gtk::Dialog> = Rc::new(builder.get_object("new_connection_dialog").unwrap());

        let connect_btn: Rc<gtk::Button> =
            Rc::new(builder.get_object("new_connection_connect_btn").unwrap());
        let cancel_btn: Rc<gtk::Button> =
            Rc::new(builder.get_object("new_connection_cancel_btn").unwrap());

        let data = Rc::new(NewConnectionDlgImpl {
            model,
            dlg,
            security_policy_combo: Rc::new(builder.get_object("security_policy_combo").unwrap()),
            message_security_mode_combo: Rc::new(
                builder.get_object("security_policy_combo").unwrap(),
            ),
            endpoint_url_text: Rc::new(builder.get_object("endpoint_url_text").unwrap()),
        });

        // Connect button
        connect_btn.connect_clicked(clone!(@weak data => move |_| {
            data.on_connect_btn_clicked();
        }));

        // Cancel button
        cancel_btn.connect_clicked(clone!(@weak data => move |_| {
            data.on_cancel_btn_clicked();
        }));

        Self { data }
    }

    pub fn show(&self) {
        self.data.show();
    }
}

impl NewConnectionDlgImpl {
    pub fn on_cancel_btn_clicked(&self) {
        self.dlg.response(gtk::ResponseType::Cancel);
    }

    pub fn on_connect_btn_clicked(&self) {
        let endpoint_url = self.endpoint_url_text.get_text().as_str().into();

        let security_policy = SecurityPolicy::from_str(
            self.security_policy_combo
                .get_active_text()
                .unwrap()
                .as_str(),
        )
        .unwrap();

        let message_security_mode = match self
            .message_security_mode_combo
            .get_active_text()
            .unwrap()
            .as_str()
        {
            "None" => MessageSecurityMode::None,
            "Sign" => MessageSecurityMode::Sign,
            "SignAndEncrypt" => MessageSecurityMode::SignAndEncrypt,
            _ => panic!("Unrecognized message security mode"),
        };

        self.model.tell(
            ModelMessage::Connect(endpoint_url, security_policy, message_security_mode),
            None,
        );
        self.dlg.response(gtk::ResponseType::Apply);
    }

    pub fn show(&self) {
        // Connect the buttons
        self.dlg.run();
        self.dlg.hide();
    }
}
