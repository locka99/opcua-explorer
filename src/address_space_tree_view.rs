use crate::model::ModelMessage;
use gtk::prelude::{BuilderExtManual, ToValue, TreeStoreExtManual};
use gtk::{TreeIter, TreeModelExt, TreePath, TreeStoreExt};
use opcua_client::prelude::*;
use riker::actor::{ActorRef, Tell};
use std::{collections::HashMap, rc::Rc, str::FromStr};

pub struct AddressSpaceTreeView {
    address_space_tree: Rc<gtk::TreeView>,
    address_space_model: Rc<gtk::TreeStore>,
    model: ActorRef<ModelMessage>,
    address_space_map: HashMap<NodeId, gtk::TreeIter>,
}

impl AddressSpaceTreeView {
    pub fn new(builder: Rc<gtk::Builder>, model: ActorRef<ModelMessage>) -> Self {
        // Address space explorer pane
        let address_space_tree: Rc<gtk::TreeView> =
            Rc::new(builder.get_object("address_space_tree").unwrap());

        // Address space explorer pane
        let address_space_model: Rc<gtk::TreeStore> =
            Rc::new(builder.get_object("address_space_model").unwrap());

        AddressSpaceTreeView {
            address_space_tree,
            model,
            address_space_model,
            address_space_map: HashMap::new(),
        }
    }

    pub fn populate(&self) {
        self.clear_address_space();
        self.model
            .send_msg(ModelMessage::BrowseNode(ObjectId::RootFolder.into()), None);
    }

    pub fn clear_address_space(&self) {
        self.address_space_model.clear();
    }

    const COL_DUMMY: u32 = 0;
    const COL_NODE_ID: u32 = 1;
    const COL_BROWSE_NAME: u32 = 2;
    const COL_DISPLAY_NAME: u32 = 3;
    const COL_REFERENCE_TYPE_ID: u32 = 4;

    pub fn on_browse_node_result(
        &mut self,
        parent_node_id: NodeId,
        browse_node_result: BrowseResult,
    ) {
        println!("browse node result");

        if browse_node_result.status_code.is_good() {
            let parent = if parent_node_id == ObjectId::RootFolder.into() {
                None
            } else if let Some(iter) = self.address_space_map.get(&parent_node_id) {
                if !self.has_dummy_node(iter) {
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

            if let Some(references) = browse_node_result.references {
                references.iter().for_each(|r| {
                    self.insert_reference(r, parent.clone());
                });
            }

            // Finally remove the dummy node. This isn't done first in case removal screws up expand/contract on the view.
            if let Some(parent) = parent {
                self.remove_dummy_node(&parent);
            }
        }
    }

    pub fn row_expanded(&self, iter: &TreeIter, _path: &TreePath) -> bool {
        println!("address_view_row_expanded");

        if self.has_dummy_node(iter) {
            let v = self
                .address_space_model
                .get_value(iter, Self::COL_NODE_ID as i32);
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

    fn insert_reference(&mut self, r: &ReferenceDescription, parent: Option<TreeIter>) {
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
            self.insert_with_values(parent, None, columns, &values)
        } else {
            self.insert_with_values(None, None, columns, &values)
        };

        // Insert a dummy node under the reference
        self.insert_dummy_node(i.clone());

        println!("Adding mapping between {:?} and {:?}", r.node_id.node_id, i);

        self.address_space_map
            .insert(r.node_id.node_id.clone(), i.clone());
    }

    fn insert_dummy_node(&self, parent: TreeIter) -> TreeIter {
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

        self.insert_with_values(Some(&parent), None, columns, &values)
    }

    fn has_dummy_node(&self, parent: &TreeIter) -> bool {
        if let Some(child_iter) = self.address_space_model.iter_children(Some(parent)) {
            let v = self
                .address_space_model
                .get_value(&child_iter, Self::COL_DUMMY as i32);
            let mut next_iter = true;
            while next_iter {
                if let Ok(Some(is_dummy)) = v.get::<bool>() {
                    if is_dummy {
                        return true;
                    }
                }
                next_iter = self.address_space_model.iter_next(&child_iter);
            }
        }
        false
    }

    fn remove_dummy_node(&self, parent: &TreeIter) -> bool {
        if let Some(child_iter) = self.address_space_model.iter_children(Some(parent)) {
            let v = self
                .address_space_model
                .get_value(&child_iter, Self::COL_DUMMY as i32);
            let mut next_iter = true;
            while next_iter {
                if let Ok(Some(is_dummy)) = v.get::<bool>() {
                    if is_dummy {
                        // Remove the element
                        self.address_space_model.remove(&child_iter);
                        return true;
                    }
                }
                next_iter = self.address_space_model.iter_next(&child_iter);
            }
        }

        false
    }

    fn insert_with_values(
        &self,
        parent: Option<&TreeIter>,
        position: Option<u32>,
        columns: &[u32],
        values: &[&dyn ToValue],
    ) -> TreeIter {
        self.address_space_model
            .insert_with_values(parent, position, columns, &values)
    }
}
