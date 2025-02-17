use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use serde_json::{Value, json};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use tokio::time::{sleep, Duration};
use tracing::{instrument, info};
use tracing_error::{InstrumentResult, TracedError};

use crate::utils::DistriError;

const PRUNE_INTERVAL: u64 = 1500;

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
enum Filter {
    Eq { field: String, value: Value },
    Neq { field: String, value: Value },
    Gt { field: String, value: f64 },
    Gte { field: String, value: f64 },
    Lt { field: String, value: f64 },
    Lte { field: String, value: f64 },
    Contains { field: String, value: String },
    In { field: String, values: Vec<Value> },
    And { conditions: Vec<Filter> },
    Or { conditions: Vec<Filter> },
}

/// Build a predicate function from a Filter.
/// The returned closure checks whether a given document (as a JSON Value)
/// satisfies the filter condition.
fn build_filter(filter: Filter) -> Box<dyn Fn(&Uuid, &Value) -> bool + Send + Sync> {
    match filter {
        Filter::Eq { field, value } => {
            Box::new(move |_, doc| {
                doc.as_object()
                    .and_then(|map| map.get(&field))
                    .map_or(false, |v| v == &value)
            })
        }
        Filter::Neq { field, value } => {
            Box::new(move |_, doc| {
                doc.as_object()
                    .and_then(|map| map.get(&field))
                    .map_or(false, |v| v != &value)
            })
        }
        Filter::Gt { field, value } => {
            Box::new(move |_, doc| {
                doc.as_object()
                    .and_then(|map| map.get(&field))
                    .and_then(|v| v.as_f64())
                    .map_or(false, |num| num > value)
            })
        }
        Filter::Gte { field, value } => {
            Box::new(move |_, doc| {
                doc.as_object()
                    .and_then(|map| map.get(&field))
                    .and_then(|v| v.as_f64())
                    .map_or(false, |num| num >= value)
            })
        }
        Filter::Lt { field, value } => {
            Box::new(move |_, doc| {
                doc.as_object()
                    .and_then(|map| map.get(&field))
                    .and_then(|v| v.as_f64())
                    .map_or(false, |num| num < value)
            })
        }
        Filter::Lte { field, value } => {
            Box::new(move |_, doc| {
                doc.as_object()
                    .and_then(|map| map.get(&field))
                    .and_then(|v| v.as_f64())
                    .map_or(false, |num| num <= value)
            })
        }
        Filter::Contains { field, value } => {
            Box::new(move |_, doc| {
                doc.as_object()
                    .and_then(|map| map.get(&field))
                    .and_then(|v| v.as_str())
                    .map_or(false, |s| s.contains(&value))
            })
        }
        Filter::In { field, values } => {
            Box::new(move |_, doc| {
                doc.as_object()
                    .and_then(|map| map.get(&field))
                    .map_or(false, |v| values.contains(v))
            })
        }
        Filter::And { conditions } => {
            let predicates: Vec<_> = conditions.into_iter().map(build_filter).collect();
            Box::new(move |uuid, doc| predicates.iter().all(|pred| pred(uuid, doc)))
        }
        Filter::Or { conditions } => {
            let predicates: Vec<_> = conditions.into_iter().map(build_filter).collect();
            Box::new(move |uuid, doc| predicates.iter().any(|pred| pred(uuid, doc)))
        }
    }
}


/// DBResult now owns the metadata rather than holding references.
pub struct DBResult {
    data: HashMap<Uuid, Value>, // a subset of the collection
    metadata: HashMap<Uuid, Metadata>, // Owned metadata entries
}

impl DBResult {
    pub fn new(
        data: HashMap<Uuid, Value>,
        metadata: HashMap<Uuid, Metadata>,
    ) -> DBResult {
        DBResult { data, metadata }
    }

    pub fn filter<F>(&self, predicate: F) -> DBResult
    where
        F: Fn(&Uuid, &Value) -> bool,
    {
        let filtered_data: HashMap<Uuid, Value> = self
            .data
            .iter()
            .filter(|(uuid, doc)| predicate(uuid, doc))
            .map(|(uuid, doc)| (*uuid, doc.clone()))
            .collect();

        let filtered_metadata: HashMap<Uuid, Metadata> = self
            .metadata
            .iter()
            .filter(|(uuid, _)| filtered_data.contains_key(uuid))
            .map(|(uuid, meta)| (*uuid, meta.clone()))
            .collect();

        DBResult::new(filtered_data, filtered_metadata)
    }

    /// Joins two `DBResult` sets.
    pub fn join(&self, other: DBResult) -> DBResult {
        let mut new_data = self.data.clone();
        new_data.extend(other.data);

        let mut new_metadata = self.metadata.clone();
        new_metadata.extend(other.metadata);

        DBResult { data: new_data, metadata: new_metadata }
    }
}



// Define metadata structure for CRDT
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Metadata {
    deleted: bool,
    hash: String,
    timestamp: DateTime<Utc>,
    version_number: u32,
}

#[derive(Clone)]
pub struct DB {
    collections: Arc<Mutex<HashMap<String, HashMap<Uuid, Value>>>>,
    collections_metadata: Arc<Mutex<HashMap<String, HashMap<Uuid, Metadata>>>>,
    pub data_version: Arc<Mutex<u64>>,
    pub time_to_update: Arc<Mutex<bool>>,
}

impl DB {
    pub fn new(
        collections: HashMap<String, HashMap<Uuid, Value>>,
        collections_metadata: HashMap<String, HashMap<Uuid, Metadata>>,
    ) -> Self {
        DB {
            collections: Arc::new(Mutex::new(collections)),
            collections_metadata: Arc::new(Mutex::new(collections_metadata)),
            data_version: Arc::new(Mutex::new(0)),
            time_to_update: Arc::new(Mutex::new(true)),
        }
    }

    /// Converts all metadata in the DB into a JSON string.
    pub async fn get_metadata_json(&self) -> Result<String, String> {
        let _ = self.data_version.lock().await; // Lock version first for consistency
        let metadata_map = self.collections_metadata.lock().await;

        // Serialize to string
        serde_json::to_string(&metadata_map.clone())
            .map_err(|e| format!("Failed to serialize metadata: {}", e))
    }

    /// Merge incoming metadata and determine which UUIDs need to be requested.
    pub async fn merge_metadata(
        &self,
        collection_name: &str,
        remote_metadata: HashMap<Uuid, Metadata>,
    ) -> Vec<Uuid> {
        let mut local_metadata = self.collections_metadata.lock().await;
        let local_collection_metadata = local_metadata
            .entry(collection_name.to_string())
            .or_insert_with(HashMap::new);

        let mut uuids_to_request = Vec::new();

        for (uuid, remote_meta) in remote_metadata {
            match local_collection_metadata.get_mut(&uuid) {
                Some(local_meta) => {
                    // CRDT Merge Logic
                    if remote_meta.version_number > local_meta.version_number
                        || (local_meta.deleted && remote_meta.timestamp > local_meta.timestamp)
                    {
                        if local_meta.deleted && !remote_meta.deleted {
                            // TODO: delete local version to allow rewriting in merge_data
                        }
                        *local_meta = remote_meta.clone();
                        if !remote_meta.deleted {
                            uuids_to_request.push(uuid);
                        }
                    } else if remote_meta.version_number == local_meta.version_number {
                        if remote_meta.timestamp > local_meta.timestamp {
                            *local_meta = remote_meta.clone();
                            if !remote_meta.deleted {
                                uuids_to_request.push(uuid);
                            }
                        }
                    } else if local_meta.deleted
                        && !remote_meta.deleted
                        && remote_meta.timestamp > local_meta.timestamp
                    {
                        *local_meta = remote_meta.clone();
                        uuids_to_request.push(uuid);
                    }
                }
                None => {
                    // New document or deletion.
                    local_collection_metadata.insert(uuid, remote_meta.clone());
                    if !remote_meta.deleted {
                        uuids_to_request.push(uuid);
                    }
                }
            }
        }

        uuids_to_request
    }

    /// Merge actual data based on the merged metadata.
    pub async fn merge_data(
        &self,
        collection_name: &str,
        incoming_docs: HashMap<Uuid, Value>,
    ) -> Result<(), String> {
        let mut collections = self.collections.lock().await;
        let mut metadata_map = self.collections_metadata.lock().await;

        let collection = collections
            .entry(collection_name.to_string())
            .or_insert_with(HashMap::new);
        let metadata_collection = metadata_map
            .entry(collection_name.to_string())
            .or_insert_with(HashMap::new);

        for (uuid, doc) in incoming_docs {
            if let Some(metadata) = metadata_collection.get(&uuid) {
                if metadata.deleted {
                    // Do not reintroduce deleted documents (tombstoning)
                    continue;
                }
                collection.insert(uuid, doc);
            }
        }

        self.increment_version().await;
        Ok(())
    }

    /// Retrieves a subset of entries along with their metadata.
    pub async fn get_entries(
        &self,
        collection_name: &str,
        entries: Vec<Uuid>,
    ) -> DBResult {
        let collections = self.collections.lock().await;
        let metadata = self.collections_metadata.lock().await;

        let mut filtered_data = HashMap::new();
        let mut filtered_metadata = HashMap::new();

        if let Some(collection_data) = collections.get(collection_name) {
            if let Some(collection_metadata) = metadata.get(collection_name) {
                for uuid in &entries {
                    if let Some(doc) = collection_data.get(uuid) {
                        if let Some(meta) = collection_metadata.get(uuid) {
                            if !meta.deleted {
                                filtered_data.insert(*uuid, doc.clone());
                                filtered_metadata.insert(*uuid, meta.clone());
                            }
                        }
                    }
                }
            }
        }

        DBResult::new(filtered_data, filtered_metadata)
    }

    /// Periodically clean up tombstoned documents after a TTL.
    pub async fn prune_deleted_docs(&self, ttl: Duration) {
        loop {
            sleep(Duration::from_secs(PRUNE_INTERVAL)).await;

            let mut collections = self.collections.lock().await;
            let mut metadata_map = self.collections_metadata.lock().await;

            for (collection_name, metadata_collection) in metadata_map.iter_mut() {
                if let Some(collection) = collections.get_mut(collection_name) {
                    let uuids_to_prune: Vec<Uuid> = metadata_collection
                        .iter()
                        .filter(|(_, meta)| {
                            meta.deleted
                                && Utc::now()
                                    .signed_duration_since(meta.timestamp)
                                    .to_std()
                                    .unwrap()
                                    > ttl
                        })
                        .map(|(uuid, _)| *uuid)
                        .collect();

                    for uuid in uuids_to_prune {
                        metadata_collection.remove(&uuid);
                        collection.remove(&uuid);
                    }
                }
            }
        }
    }

    /// Safely increments the database version, preventing overflow.
    pub async fn increment_version(&self) -> u64 {
        let mut version = self.data_version.lock().await;

        match version.checked_add(1) {
            Some(new_version) => {
                *version = new_version;
            }
            None => {
                eprintln!("Warning: Database version overflow detected. Resetting to 0.");
                *version = 0;
            }
        }
        version.clone()
    }

    /// DB Operations
    #[instrument(name = "Add Doc", skip(self, entry), fields(collection = collection_name))]
    pub async fn add_doc(
        &self,
        collection_name: &str,
        mut entry: Value,
    ) -> Result<String, TracedError<DistriError>> {
        let uuid = if let Value::Object(ref mut obj) = entry {
            if let Some(Value::String(existing_uuid)) = obj.get("UUID") {
                Uuid::parse_str(existing_uuid).unwrap_or_else(|_| Uuid::new_v4())
            } else {
                let new_uuid = Uuid::new_v4();
                obj.insert("UUID".to_string(), Value::String(new_uuid.to_string()));
                new_uuid
            }
        } else {
            return Err(DistriError::ValidationError(
                "Entry must be a JSON object".to_string(),
            ))
            .in_current_span();
        };

        let mut collections = self.collections.lock().await;
        let mut metadata = self.collections_metadata.lock().await;

        let collection = collections
            .entry(collection_name.to_string())
            .or_insert_with(HashMap::new);
        let collection_metadata = metadata
            .entry(collection_name.to_string())
            .or_insert_with(HashMap::new);

        collection.insert(uuid, entry);

        // Update Metadata with CRDT fields
        let new_metadata = Metadata {
            deleted: false,
            hash: "dummy_hash".to_string(), // Replace with actual hash calculation
            timestamp: Utc::now(),
            version_number: 1,
        };
        collection_metadata.insert(uuid, new_metadata);

        info!("Added Doc with UUID: {}", uuid);
        Ok(uuid.to_string())
    }

    #[instrument(name = "Update Doc", skip(self, entry), fields(collection = collection_name))]
    pub async fn update_doc(
        &self,
        collection_name: &str,
        entry: Value,
    ) -> Result<String, TracedError<DistriError>> {
        let uuid = if let Value::Object(ref obj) = entry {
            if let Some(Value::String(id)) = obj.get("UUID") {
                Uuid::parse_str(id)
                    .map_err(|_| DistriError::ValidationError("Invalid UUID format".to_string()))?
            } else {
                return Err(DistriError::ValidationError(
                    "No 'UUID' field found in JSON".to_string(),
                ))
                .in_current_span();
            }
        } else {
            return Err(DistriError::ValidationError("JSON must be an object".to_string()))
                .in_current_span();
        };

        let mut collections = self.collections.lock().await;
        let mut metadata = self.collections_metadata.lock().await;

        let collection = collections
            .entry(collection_name.to_string())
            .or_insert_with(HashMap::new);
        let collection_metadata = metadata
            .entry(collection_name.to_string())
            .or_insert_with(HashMap::new);

        if collection.contains_key(&uuid) {
            collection.insert(uuid, entry);

            // Update metadata
            if let Some(meta) = collection_metadata.get_mut(&uuid) {
                meta.timestamp = Utc::now();
                meta.version_number += 1;
            }

            Ok("Document updated successfully".to_string())
        } else {
            self.add_doc(collection_name, entry).await
        }
    }

    #[instrument(name = "Delete Doc", skip(self, entry), fields(collection = collection_name))]
    pub async fn delete_doc(
        &self,
        collection_name: &str,
        entry: Value,
    ) -> Result<String, TracedError<DistriError>> {
        let uuid = if let Value::Object(ref obj) = entry {
            if let Some(Value::String(id)) = obj.get("UUID") {
                Uuid::parse_str(id)
                    .map_err(|_| DistriError::ValidationError("Invalid UUID format".to_string()))?
            } else {
                return Err(DistriError::ValidationError(
                    "No 'UUID' field found in JSON".to_string(),
                ))
                .in_current_span();
            }
        } else {
            return Err(DistriError::ValidationError("JSON must be an object".to_string()))
                .in_current_span();
        };

        let mut metadata = self.collections_metadata.lock().await;

        if let Some(collection_metadata) = metadata.get_mut(collection_name) {
            if let Some(meta) = collection_metadata.get_mut(&uuid) {
                meta.deleted = true;
                meta.timestamp = Utc::now();
                meta.version_number += 1;

                Ok("Document marked as deleted.".to_string())
            } else {
                Err(DistriError::ValidationError("Document does not exist.".to_string()))
                    .in_current_span()
            }
        } else {
            Err(DistriError::ValidationError(format!(
                "Collection '{}' does not exist.",
                collection_name
            )))
            .in_current_span()
        }
    }

    #[instrument(name = "Add Collection", skip(self), fields(collection = collection_name))]
    pub async fn add_collection(
        &self,
        collection_name: &str,
    ) -> Result<(), TracedError<DistriError>> {
        let _lock = self.data_version.lock().await;
        let mut collections = self.collections.lock().await;
        let mut metadata = self.collections_metadata.lock().await;
        collections.insert(collection_name.to_string(), HashMap::new());
        metadata.insert(collection_name.to_string(), HashMap::new());

        Ok(())
    }

    pub async fn get_stats(&self) -> Vec<String> {
        self.collections
            .lock()
            .await
            .clone()
            .iter()
            .map(|(name, entries)| format!("{}: {} entries", name, entries.len()))
            .collect::<Vec<_>>()
    }
}
