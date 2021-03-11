mod index_actor;
mod update_actor;
mod update_handler;
mod update_store;
mod updates;
mod uuid_resolver;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use actix_web::web::{Bytes, Payload};
use futures::stream::StreamExt;
use milli::update::{IndexDocumentsMethod, UpdateFormat};
use serde::{Serialize, Deserialize};
use tokio::sync::{mpsc, oneshot};
use tokio::time::sleep;
use uuid::Uuid;

pub use updates::{Processed, Processing, Failed};
use crate::index::{SearchResult, SearchQuery, Document};
use crate::index::{UpdateResult, Settings, Facets};

pub type UpdateStatus = updates::UpdateStatus<UpdateMeta, UpdateResult, String>;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IndexMetadata {
    name: String,
    #[serde(flatten)]
    meta: index_actor::IndexMeta,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum UpdateMeta {
    DocumentsAddition {
        method: IndexDocumentsMethod,
        format: UpdateFormat,
        primary_key: Option<String>,
    },
    ClearDocuments,
    DeleteDocuments,
    Settings(Settings),
    Facets(Facets),
}

#[derive(Clone, Debug)]
pub struct IndexSettings {
    pub name: Option<String>,
    pub primary_key: Option<String>,
}


pub struct IndexController {
    uuid_resolver: uuid_resolver::UuidResolverHandle,
    index_handle: index_actor::IndexActorHandle,
    update_handle: update_actor::UpdateActorHandle<Bytes>,
}

enum IndexControllerMsg {
    CreateIndex {
        uuid: Uuid,
        primary_key: Option<String>,
        ret: oneshot::Sender<anyhow::Result<IndexMetadata>>,
    },
    Shutdown,
}

impl IndexController {
    pub fn new(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let uuid_resolver = uuid_resolver::UuidResolverHandle::new(&path)?;
        let index_actor = index_actor::IndexActorHandle::new(&path)?;
        let update_handle = update_actor::UpdateActorHandle::new(index_actor.clone(), &path)?;
        Ok(Self { uuid_resolver, index_handle: index_actor, update_handle })
    }

    pub async fn add_documents(
        &self,
        index: String,
        method: milli::update::IndexDocumentsMethod,
        format: milli::update::UpdateFormat,
        mut payload: Payload,
        primary_key: Option<String>,
    ) -> anyhow::Result<UpdateStatus> {
        let uuid = self.uuid_resolver.get_or_create(index).await?;
        let meta = UpdateMeta::DocumentsAddition { method, format, primary_key };
        let (sender, receiver) = mpsc::channel(10);

        // It is necessary to spawn a local task to senf the payload to the update handle to
        // prevent dead_locking between the update_handle::update that waits for the update to be
        // registered and the update_actor that waits for the the payload to be sent to it.
        tokio::task::spawn_local(async move {
            while let Some(bytes) = payload.next().await {
                match bytes {
                    Ok(bytes) => { sender.send(Ok(bytes)).await; },
                    Err(e) => {
                        let error: Box<dyn std::error::Error + Sync + Send + 'static> = Box::new(e);
                        sender.send(Err(error)).await; },
                }
            }
        });

        // This must be done *AFTER* spawning the task.
        let status = self.update_handle.update(meta, receiver, uuid).await?;
        Ok(status)
    }

    pub async fn clear_documents(&self, index: String) -> anyhow::Result<UpdateStatus> {
        let uuid = self.uuid_resolver.resolve(index).await?;
        let meta = UpdateMeta::ClearDocuments;
        let (_, receiver) = mpsc::channel(1);
        let status = self.update_handle.update(meta, receiver, uuid).await?;
        Ok(status)
    }

    pub async fn delete_documents(&self, index: String, document_ids: Vec<String>) -> anyhow::Result<UpdateStatus> {
        let uuid = self.uuid_resolver.resolve(index).await?;
        let meta = UpdateMeta::DeleteDocuments;
        let (sender, receiver) = mpsc::channel(10);

        tokio::task::spawn(async move {
            let json = serde_json::to_vec(&document_ids).unwrap();
            let bytes = Bytes::from(json);
            let _ = sender.send(Ok(bytes)).await;
        });

        let status = self.update_handle.update(meta, receiver, uuid).await?;
        Ok(status)
    }

    pub async fn update_settings(&self, index_uid: String, settings: Settings, create: bool) -> anyhow::Result<UpdateStatus> {
        let uuid = if create {
            self.uuid_resolver.get_or_create(index_uid).await?
        } else {
            self.uuid_resolver.resolve(index_uid).await?
        };
        let meta = UpdateMeta::Settings(settings);
        // Nothing so send, drop the sender right away, as not to block the update actor.
        let (_, receiver) = mpsc::channel(1);

        let status = self.update_handle.update(meta, receiver, uuid).await?;
        Ok(status)
    }

    pub async fn create_index(&self, index_settings: IndexSettings) -> anyhow::Result<IndexMetadata> {
        let IndexSettings { name, primary_key } = index_settings;
        let name = name.unwrap();
        let uuid = self.uuid_resolver.create(name.clone()).await?;
        let meta = self.index_handle.create_index(uuid, primary_key).await?;
        let _ = self.update_handle.create(uuid).await?;
        let meta = IndexMetadata { name, meta };

        Ok(meta)
    }

    pub async fn delete_index(&self, index_uid: String) -> anyhow::Result<()> {
        let uuid = self.uuid_resolver
            .delete(index_uid)
            .await?;
        self.update_handle.delete(uuid.clone()).await?;
        self.index_handle.delete(uuid).await?;
        Ok(())
    }

    pub async fn update_status(&self, index: String, id: u64) -> anyhow::Result<Option<UpdateStatus>> {
        let uuid = self.uuid_resolver
            .resolve(index)
            .await?;
        let result = self.update_handle.update_status(uuid, id).await?;
        Ok(result)
    }

    pub async fn all_update_status(&self, index: String) -> anyhow::Result<Vec<UpdateStatus>> {
        let uuid = self.uuid_resolver
            .resolve(index).await?;
        let result = self.update_handle.get_all_updates_status(uuid).await?;
        Ok(result)
    }

    pub async fn list_indexes(&self) -> anyhow::Result<Vec<IndexMetadata>> {
        let uuids = self.uuid_resolver.list().await?;

        let mut ret = Vec::new();

        for (name, uuid) in uuids {
            if let Some(meta) = self.index_handle.get_index_meta(uuid).await? {
                let meta = IndexMetadata { name, meta };
                ret.push(meta);
            }
        }

        Ok(ret)
    }

    pub async fn settings(&self, index: String) -> anyhow::Result<Settings> {
        let uuid = self.uuid_resolver
            .resolve(index.clone())
            .await?;
        let settings = self.index_handle.settings(uuid).await?;
        Ok(settings)
    }

    pub async fn documents(
        &self,
        index: String,
        offset: usize,
        limit: usize,
        attributes_to_retrieve: Option<Vec<String>>,
    ) -> anyhow::Result<Vec<Document>> {
        let uuid = self.uuid_resolver
            .resolve(index.clone())
            .await?;
        let documents = self.index_handle.documents(uuid, offset, limit, attributes_to_retrieve).await?;
        Ok(documents)
    }

    pub async fn document(
        &self,
        index: String,
        doc_id: String,
        attributes_to_retrieve: Option<Vec<String>>,
    ) -> anyhow::Result<Document> {
        let uuid = self.uuid_resolver
            .resolve(index.clone())
            .await?;
        let document = self.index_handle.document(uuid, doc_id, attributes_to_retrieve).await?;
        Ok(document)
    }

    fn update_index(&self, name: String, index_settings: IndexSettings) -> anyhow::Result<IndexMetadata> {
        todo!()
    }

    pub async fn search(&self, name: String, query: SearchQuery) -> anyhow::Result<SearchResult> {
        let uuid = self.uuid_resolver.resolve(name).await?;
        let result = self.index_handle.search(uuid, query).await?;
        Ok(result)
    }

    pub async fn get_index(&self, name: String) -> anyhow::Result<Option<IndexMetadata>> {
        let uuid = self.uuid_resolver.resolve(name.clone()).await?;
        let result = self.index_handle
            .get_index_meta(uuid)
            .await?
            .map(|meta| IndexMetadata { name, meta });
        Ok(result)
    }
}

pub async fn get_arc_ownership_blocking<T>(mut item: Arc<T>) -> T {
    loop {
        match Arc::try_unwrap(item) {
            Ok(item) => return item,
            Err(item_arc) => {
                item = item_arc;
                sleep(Duration::from_millis(100)).await;
                continue;
            }
        }
    }
}