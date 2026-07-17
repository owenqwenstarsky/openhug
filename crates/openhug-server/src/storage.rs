use std::sync::Arc;

use anyhow::{Context, Result};
use bytes::Bytes;
use object_store::{ObjectStore, ObjectStoreExt, path::Path};
use sha2::{Digest, Sha256};

use crate::config::StorageConfig;

#[derive(Clone)]
pub struct BlobStore {
    inner: Arc<dyn ObjectStore>,
}

impl BlobStore {
    pub fn from_config(config: &StorageConfig) -> Result<Self> {
        let inner: Arc<dyn ObjectStore> = match config {
            StorageConfig::Local { path } => {
                std::fs::create_dir_all(path).context("create local storage directory")?;
                Arc::new(
                    object_store::local::LocalFileSystem::new_with_prefix(path)
                        .context("initialize local storage")?,
                )
            }
            StorageConfig::S3 {
                bucket,
                region,
                endpoint,
                access_key,
                secret_key,
                virtual_hosted_style,
                ..
            } => {
                let mut builder = object_store::aws::AmazonS3Builder::new()
                    .with_bucket_name(bucket)
                    .with_region(region)
                    .with_access_key_id(access_key)
                    .with_secret_access_key(secret_key)
                    .with_virtual_hosted_style_request(*virtual_hosted_style)
                    .with_allow_http(
                        endpoint
                            .as_deref()
                            .is_some_and(|e| e.starts_with("http://")),
                    );
                if let Some(endpoint) = endpoint {
                    builder = builder.with_endpoint(endpoint);
                }
                Arc::new(
                    builder
                        .build()
                        .context("initialize S3-compatible storage")?,
                )
            }
        };
        Ok(Self { inner })
    }

    pub async fn put(&self, bytes: Bytes) -> Result<(String, i64)> {
        let digest = hex::encode(Sha256::digest(&bytes));
        let key = blob_key(&digest);
        match self.inner.head(&Path::from(key.as_str())).await {
            Ok(_) => {}
            Err(object_store::Error::NotFound { .. }) => {
                self.inner
                    .put(&Path::from(key.as_str()), bytes.clone().into())
                    .await?;
            }
            Err(error) => return Err(error.into()),
        }
        Ok((digest, bytes.len() as i64))
    }

    pub async fn get(&self, digest: &str) -> Result<Bytes> {
        Ok(self
            .inner
            .get(&Path::from(blob_key(digest)))
            .await?
            .bytes()
            .await?)
    }

    pub async fn healthcheck(&self) -> Result<()> {
        let _ = self.inner.list_with_delimiter(None).await?;
        Ok(())
    }

    pub async fn contains(&self, digest: &str) -> Result<bool> {
        match self.inner.head(&Path::from(blob_key(digest))).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }

    pub async fn delete(&self, digest: &str) -> Result<()> {
        match self.inner.delete(&Path::from(blob_key(digest))).await {
            Ok(_) | Err(object_store::Error::NotFound { .. }) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

pub fn blob_key(digest: &str) -> String {
    format!("blobs/{}/{digest}", &digest[..2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_keys_are_sharded() {
        let digest = "ab00000000000000000000000000000000000000000000000000000000000000";
        assert_eq!(blob_key(digest), format!("blobs/ab/{digest}"));
    }
}
