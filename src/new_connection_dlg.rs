use std::rc::Rc;

use glib::clone;
use gtk::{self, prelude::*};

use opcua_client::prelude::*;

use crate::app_model::AppModel;

struct NewConnectionDlgImpl {
    model: Rc<AppModel>,
    dlg: Rc<gtk::Dialog>,
    security_policy_combo: Rc<gtk::ComboBox>,
    message_security_mode_combo: Rc<gtk::ComboBox>,
}

pub(crate) struct NewConnectionDlg {
    data: Rc<NewConnectionDlgImpl>,
}

impl NewConnectionDlg {
    pub fn new(model: Rc<AppModel>, builder: &gtk::Builder) -> Self {
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
        self.model.connect(
            "opc.tcp://fixme",
            SecurityPolicy::None,
            MessageSecurityMode::None,
        );
        self.dlg.response(gtk::ResponseType::Apply);
    }

    pub fn show(&self) {
        // Connect the buttons
        self.dlg.run();
        self.dlg.hide();
    }
}
