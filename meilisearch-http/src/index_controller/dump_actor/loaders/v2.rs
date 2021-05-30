use std::path::Path;

use anyhow::Context;
use chrono::{DateTime, Utc};
use log::{info, warn};
use serde::{Deserialize, Serialize};

use crate::{index::Index, index_controller::{update_actor::UpdateStore, uuid_resolver::HeedUuidStore}, option::IndexerOpts};

#[derive(Serialize, Deserialize, Debug)]
pub struct MetadataV2 {
    db_version: String,
    index_db_size: u64,
    update_db_size: u64,
    dump_date: DateTime<Utc>,
}

impl MetadataV2 {
    pub fn new(index_db_size: u64, update_db_size: u64) -> Self {
        Self {
            db_version: env!("CARGO_PKG_VERSION").to_string(),
            index_db_size,
            update_db_size,
            dump_date: Utc::now(),
        }
    }

    pub fn load_dump(
        self,
        src: impl AsRef<Path>,
        dst: impl AsRef<Path>,
        _index_db_size: u64,
        _update_db_size: u64,
        indexing_options: &IndexerOpts,
    ) -> anyhow::Result<()> {
        info!(
            "Loading dump from {}, dump database version: {}, dump version: V2",
            self.dump_date, self.db_version
        );
        // get dir in which to load the db:
        let dst_dir = dst
            .as_ref()
            .parent()
            .with_context(|| format!("Invalid db path: {}", dst.as_ref().display()))?;

        let tmp_dst = tempfile::tempdir_in(dst_dir)?;

        info!("Loading index database.");
        HeedUuidStore::load_dump(src.as_ref(), &tmp_dst)?;

        info!("Loading updates.");
        UpdateStore::load_dump(&src, &tmp_dst, self.update_db_size)?;

        info!("Loading indexes");
        let indexes_path = src.as_ref().join("indexes");
        let indexes = indexes_path.read_dir()?;
        for index in indexes {
            let index = index?;
            Index::load_dump(&index.path(), &tmp_dst, self.index_db_size, indexing_options)?;
        }

        // Persist and atomically rename the db
        let persisted_dump = tmp_dst.into_path();
        if dst.as_ref().exists() {
            warn!("Overwriting database at {}", dst.as_ref().display());
            std::fs::remove_dir_all(&dst)?;
        }

        std::fs::rename(&persisted_dump, &dst)?;

        Ok(())
    }
}