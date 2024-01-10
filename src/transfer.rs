use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;

use collider_core::id::FamilyId;
use collider_core::component::ComponentType;

#[derive(Debug, Clone)]
pub struct TransferGraph {
    links: Arc<RwLock<HashMap<ComponentType, TransferEdge>>>,
}

impl TransferGraph {
    pub fn new() -> Self {
        Self {
            links: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn get(&self, component: &ComponentType) -> Option<TransferEdge> {
        match self.links.read() {
            Ok(graph) => graph.get(component).cloned(),
            Err(e) => {
                panic!("unable to read transfer graph - rwlock - {:?}", e)
            }
        }
    }

    pub fn set(&self, component: &ComponentType, edge: TransferEdge) -> Option<TransferEdge> {
        match self.links.write() {
            Ok(mut graph) => graph.insert(*component, edge),
            Err(e) => {
                panic!("unable to set transfer graph - rwlock - {:?}", e)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum TransferEdge {
    Add(FamilyId),
    Remove(FamilyId),
}
