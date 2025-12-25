use setu_types::{Event, EventId, EventStatus, SetuResult};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct EventStore {
    events: Arc<RwLock<HashMap<EventId, Event>>>,
    by_creator: Arc<RwLock<HashMap<String, Vec<EventId>>>>,
    by_status: Arc<RwLock<HashMap<EventStatus, Vec<EventId>>>>,
}

impl EventStore {
    pub fn new() -> Self {
        Self {
            events: Arc::new(RwLock::new(HashMap::new())),
            by_creator: Arc::new(RwLock::new(HashMap::new())),
            by_status: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn store(&self, event: Event) -> SetuResult<()> {
        let event_id = event.id.clone();
        let creator = event.creator.clone();
        let status = event.status;

        let mut events = self.events.write().await;
        events.insert(event_id.clone(), event);

        let mut by_creator = self.by_creator.write().await;
        by_creator
            .entry(creator)
            .or_insert_with(Vec::new)
            .push(event_id.clone());

        let mut by_status = self.by_status.write().await;
        by_status
            .entry(status)
            .or_insert_with(Vec::new)
            .push(event_id);

        Ok(())
    }

    pub async fn get(&self, event_id: &EventId) -> Option<Event> {
        let events = self.events.read().await;
        events.get(event_id).cloned()
    }

    pub async fn get_many(&self, event_ids: &[EventId]) -> Vec<Event> {
        let events = self.events.read().await;
        event_ids
            .iter()
            .filter_map(|id| events.get(id).cloned())
            .collect()
    }

    pub async fn update_status(&self, event_id: &EventId, new_status: EventStatus) {
        let old_status = {
            let mut events = self.events.write().await;
            if let Some(event) = events.get_mut(event_id) {
                let old = event.status;
                event.status = new_status;
                Some(old)
            } else {
                None
            }
        };

        if let Some(old_status) = old_status {
            let mut by_status = self.by_status.write().await;
            
            if let Some(ids) = by_status.get_mut(&old_status) {
                ids.retain(|id| id != event_id);
            }
            
            by_status
                .entry(new_status)
                .or_insert_with(Vec::new)
                .push(event_id.clone());
        }
    }

    pub async fn get_by_creator(&self, creator: &str) -> Vec<Event> {
        let by_creator = self.by_creator.read().await;
        let events = self.events.read().await;

        by_creator
            .get(creator)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| events.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn get_by_status(&self, status: EventStatus) -> Vec<Event> {
        let by_status = self.by_status.read().await;
        let events = self.events.read().await;

        by_status
            .get(&status)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| events.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn count(&self) -> usize {
        self.events.read().await.len()
    }

    pub async fn count_by_status(&self, status: EventStatus) -> usize {
        let by_status = self.by_status.read().await;
        by_status.get(&status).map(|v| v.len()).unwrap_or(0)
    }

    pub async fn exists(&self, event_id: &EventId) -> bool {
        self.events.read().await.contains_key(event_id)
    }
}

impl Clone for EventStore {
    fn clone(&self) -> Self {
        Self {
            events: Arc::clone(&self.events),
            by_creator: Arc::clone(&self.by_creator),
            by_status: Arc::clone(&self.by_status),
        }
    }
}

impl Default for EventStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use setu_types::{EventType, VectorClock, VLCSnapshot};

    fn create_event(creator: &str) -> Event {
        Event::new(
            EventType::Transfer,
            vec![],
            VLCSnapshot {
                vector_clock: VectorClock::new(),
                logical_time: 1,
                physical_time: 1000,
            },
            creator.to_string(),
        )
    }

    #[tokio::test]
    async fn test_store_and_get() {
        let store = EventStore::new();
        let event = create_event("node1");
        let event_id = event.id.clone();

        store.store(event).await.unwrap();

        let retrieved = store.get(&event_id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, event_id);
    }

    #[tokio::test]
    async fn test_get_by_creator() {
        let store = EventStore::new();
        
        store.store(create_event("node1")).await.unwrap();
        store.store(create_event("node1")).await.unwrap();
        store.store(create_event("node2")).await.unwrap();

        let node1_events = store.get_by_creator("node1").await;
        assert_eq!(node1_events.len(), 2);
    }

    #[tokio::test]
    async fn test_update_status() {
        let store = EventStore::new();
        let event = create_event("node1");
        let event_id = event.id.clone();

        store.store(event).await.unwrap();
        store.update_status(&event_id, EventStatus::Executed).await;

        let updated = store.get(&event_id).await.unwrap();
        assert_eq!(updated.status, EventStatus::Executed);
    }
}
