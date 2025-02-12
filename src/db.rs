use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use serde_json::Value;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use tokio::time::{sleep, Duration};
use tracing::{instrument, info};
use tracing_error::{InstrumentResult, TracedError};

use crate::utils::DistriError;

const PRUNE_INTERVAL:u64 = 1500;


// Define metadata structure for CRDT
#[derive(Clone, Debug)]
struct Metadata {
    deleted: bool,
    hash: String,
    timestamp: DateTime<Utc>,
    version_number: u32,
}

#[derive(Clone)]
pub struct DB {
    collections: Arc<Mutex<HashMap<String, HashMap<Uuid, Value>>>>,
    collections_metadata: Arc<Mutex<HashMap<String, HashMap<Uuid, Metadata>>>>,
    data_version: Arc<Mutex<u64>>,
    time_to_update: Arc<Mutex<bool>>,
}

impl DB {
    pub fn new() -> Self {
        DB {
            collections: Arc::new(Mutex::new(HashMap::new())),
            collections_metadata: Arc::new(Mutex::new(HashMap::new())),
            data_version: Arc::new(Mutex::new(0)),
            time_to_update: Arc::new(Mutex::new(true)),
        }
    }

   /// Merge incoming metadata and determine which UUIDs need to be requested.
   pub async fn merge_metadata(&self, collection_name: &str, remote_metadata: HashMap<Uuid, Metadata>) -> Vec<Uuid> {
        let mut local_metadata = self.collections_metadata.lock().await;
        let local_collection_metadata = local_metadata.entry(collection_name.to_string())
                                                    .or_insert_with(HashMap::new);

        let mut uuids_to_request = Vec::new();

        for (uuid, remote_meta) in remote_metadata {
            match local_collection_metadata.get_mut(&uuid) {  // Use get_mut to get a mutable reference
                Some(local_meta) => {
                    // CRDT Merge Logic
                    if remote_meta.version_number > local_meta.version_number || local_meta.deleted && remote_meta.timestamp > local_meta.timestamp {
                        // Remote version is newer (update or delete)
                        if local_meta.deleted && !remote_meta.deleted {
                            // TODO delete local version to allow rewriting in merge_data
                        }
                        *local_meta = remote_meta.clone();  // Now this works because local_meta is mutable
                        
                        if !remote_meta.deleted {
                            uuids_to_request.push(uuid);  // We need the updated data
                        }
                    } else if remote_meta.version_number == local_meta.version_number {
                        // Same version, compare timestamps
                        if remote_meta.timestamp > local_meta.timestamp {
                            *local_meta = remote_meta.clone();
        
                            if !remote_meta.deleted {
                                uuids_to_request.push(uuid);  // We need the updated data
                            }
                        }
                    } else if local_meta.deleted && !remote_meta.deleted && remote_meta.timestamp > local_meta.timestamp {
                        // replace local version to allow rewriting in merge_data
                        *local_meta = remote_meta.clone(); 
                        uuids_to_request.push(uuid);
                    }
                    // Else: Local version is newer or same, no action needed
                },
                None => {
                    // We don't have this document at all (new addition or deletion)
                    local_collection_metadata.insert(uuid, remote_meta.clone());
        
                    if !remote_meta.deleted {
                        uuids_to_request.push(uuid);  // New doc, request data
                    }
                }
            }
        }

        uuids_to_request
    }

    /// Merge actual data based on the merged metadata.
    pub async fn merge_data(&self, collection_name: &str, incoming_docs: HashMap<Uuid, Value>) -> Result<(), String> {
        let mut collections = self.collections.lock().await;
        let mut metadata_map = self.collections_metadata.lock().await;

        let collection = collections.entry(collection_name.to_string()).or_insert_with(HashMap::new);
        let metadata_collection = metadata_map.entry(collection_name.to_string()).or_insert_with(HashMap::new);

        for (uuid, doc) in incoming_docs {
            if let Some(metadata) = metadata_collection.get(&uuid) {
                if metadata.deleted {
                    // Do not reintroduce deleted documents (tombstoning)
                    continue;
                }

                // Update document if metadata matches (already validated via merge_metadata)
                collection.insert(uuid, doc);
            }
        }

        self.increment_version().await;
        Ok(())
    }

    /// Periodically clean up tombstoned documents after a TTL.
    pub async fn prune_deleted_docs(&self, ttl: Duration) {
        loop {
            sleep(Duration::from_secs(PRUNE_INTERVAL)).await;  // Run every hour

            let mut collections = self.collections.lock().await;
            let mut metadata_map = self.collections_metadata.lock().await;

            for (collection_name, metadata_collection) in metadata_map.iter_mut() {
                let collection = collections.get_mut(collection_name).unwrap();

                let uuids_to_prune: Vec<Uuid> = metadata_collection.iter()
                    .filter(|(_, meta)| meta.deleted && Utc::now().signed_duration_since(meta.timestamp).to_std().unwrap() > ttl)
                    .map(|(uuid, _)| *uuid)
                    .collect();

                for uuid in uuids_to_prune {
                    metadata_collection.remove(&uuid);
                    collection.remove(&uuid);
                }
            }
        }
    }

    /// Safely increments the database version, preventing overflow.
    pub async fn increment_version(&self) -> u64{
        let mut version = self.data_version.lock().await;

        match version.checked_add(1) {
            Some(new_version) => {
                *version = new_version;
            },
            None => {
                // Handle overflow: Reset to 0 or log an error
                eprintln!("Warning: Database version overflow detected. Resetting to 0.");
                *version = 0;
            }
        }
        return version.clone();
    }

    /// DB Operations
    #[instrument(name = "Add Doc", skip(self, entry), fields(collection = collection_name))]
    pub async fn add_doc(&self, collection_name: &str, mut entry: Value) -> Result<String, TracedError<DistriError>> {
        let uuid = if let Value::Object(ref mut obj) = entry {
            if let Some(Value::String(existing_uuid)) = obj.get("UUID") {
                Uuid::parse_str(existing_uuid).unwrap_or_else(|_| Uuid::new_v4())
            } else {
                let new_uuid = Uuid::new_v4();
                obj.insert("UUID".to_string(), Value::String(new_uuid.to_string()));
                new_uuid
            }
        } else {
            return Err(DistriError::ValidationError("Entry must be a JSON object".to_string())).in_current_span();
        };

        let mut collections = self.collections.lock().await;
        let mut metadata = self.collections_metadata.lock().await;

        let collection = collections.entry(collection_name.to_string()).or_insert_with(HashMap::new);
        let collection_metadata = metadata.entry(collection_name.to_string()).or_insert_with(HashMap::new);

        collection.insert(uuid, entry);

        // Update Metadata with CRDT fields
        let new_metadata = Metadata {
            deleted: false,
            hash: "dummy_hash".to_string(),  // Replace with actual hash calculation
            timestamp: Utc::now(),
            version_number: 1,
        };
        collection_metadata.insert(uuid, new_metadata);

        info!("Added Doc with UUID: {}", uuid);
        Ok(uuid.to_string())
    }

    #[instrument(name = "Update Doc", skip(self, entry), fields(collection = collection_name))]
    pub async fn update_doc(&self, collection_name: &str, entry: Value) -> Result<String, TracedError<DistriError>> {
        let uuid = if let Value::Object(ref obj) = entry {
            if let Some(Value::String(id)) = obj.get("UUID") {
                Uuid::parse_str(id).map_err(|_| DistriError::ValidationError("Invalid UUID format".to_string()))?
            } else {
                return Err(DistriError::ValidationError("No 'UUID' field found in JSON".to_string())).in_current_span();
            }
        } else {
            return Err(DistriError::ValidationError("JSON must be an object".to_string())).in_current_span();
        };

        let mut collections = self.collections.lock().await;
        let mut metadata = self.collections_metadata.lock().await;

        let collection = collections.entry(collection_name.to_string()).or_insert_with(HashMap::new);
        let collection_metadata = metadata.entry(collection_name.to_string()).or_insert_with(HashMap::new);

        if collection.contains_key(&uuid) {
            collection.insert(uuid, entry);

            // Update metadata
            if let Some(meta) = collection_metadata.get_mut(&uuid) {
                meta.timestamp = Utc::now();
                meta.version_number = meta.version_number + 1; // unsafe max uint
            }

            Ok("Document updated successfully".to_string())
        } else {
            self.add_doc(collection_name, entry).await
        }
    }

    #[instrument(name = "Delete Doc", skip(self, entry), fields(collection = collection_name))]
    pub async fn delete_doc(&self, collection_name: &str, entry: Value) -> Result<String, TracedError<DistriError>> {
        let uuid = if let Value::Object(ref obj) = entry {
            if let Some(Value::String(id)) = obj.get("UUID") {
                Uuid::parse_str(id).map_err(|_| DistriError::ValidationError("Invalid UUID format".to_string()))?
            } else {
                return Err(DistriError::ValidationError("No 'UUID' field found in JSON".to_string())).in_current_span();
            }
        } else {
            return Err(DistriError::ValidationError("JSON must be an object".to_string())).in_current_span();
        };

        let mut metadata = self.collections_metadata.lock().await;

        if let Some(collection_metadata) = metadata.get_mut(collection_name) {
            if let Some(meta) = collection_metadata.get_mut(&uuid) {
                meta.deleted = true;
                meta.timestamp = Utc::now();
                meta.version_number = meta.version_number + 1;

                Ok("Document marked as deleted.".to_string())
            } else {
                Err(DistriError::ValidationError("Document does not exist.".to_string())).in_current_span()
            }
        } else {
            Err(DistriError::ValidationError(format!("Collection '{}' does not exist.", collection_name))).in_current_span()
        }
    }

    #[instrument(name = "Read Docs", skip(self, entry), fields(collection = collection_name))]
    pub async fn read_docs(&self, collection_name: &str, entry: Value) -> Result<String, TracedError<DistriError>> {
        let entry_object = entry.as_object().ok_or_else(|| {
            DistriError::ValidationError("Entry must be a JSON object".to_string())
        }).in_current_span()?;

        let collections = self.collections.lock().await;
        let metadata = self.collections_metadata.lock().await;

        if let Some(table) = collections.get(collection_name) {
            let empty_metadata = HashMap::new();
            let collection_metadata = metadata.get(collection_name).unwrap_or(&empty_metadata);

            let filtered_entries: Vec<Value> = table.iter()
                .filter(|(uuid, _)| {
                    // Only include entries not marked as deleted
                    collection_metadata.get(uuid).map_or(true, |meta| !meta.deleted)
                })
                .filter(|(_, existing_entry)| {
                    existing_entry.as_object().map_or(false, |existing_fields| {
                        entry_object.iter().all(|(key, value)| {
                            existing_fields.get(key) == Some(value)
                        })
                    })
                })
                .map(|(_, entry)| entry.clone())
                .collect();

            Ok(Value::Array(filtered_entries).to_string())
        } else {
            Err(DistriError::ValidationError(format!("Collection '{}' does not exist.", collection_name))).in_current_span()
        }
    }
}