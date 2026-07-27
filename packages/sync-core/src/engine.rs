use std::{fs, sync::Arc};

use anyhow::{Context, Result, bail};
use reqwest::{Body, Client};
use serde::Serialize;
use tempfile::NamedTempFile;
use tokio_util::io::ReaderStream;

use crate::{
    archive,
    config::{find_folder, find_peer},
    manifest,
    model::{AckRequest, ApplyResult, Config, Manifest, PlanDecision, SyncAction, SyncPlan},
    state::StateStore,
};

#[derive(Clone)]
pub struct Engine {
    pub config: Arc<Config>,
    pub state: StateStore,
    client: Client,
}

impl Engine {
    pub fn new(config: Config) -> Result<Self> {
        fs::create_dir_all(&config.data_dir).with_context(|| {
            format!(
                "failed to create data directory {}",
                config.data_dir.display()
            )
        })?;
        let state = StateStore::load(&config.data_dir)?;
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(30 * 60))
            .build()?;
        Ok(Self {
            config: Arc::new(config),
            state,
            client,
        })
    }

    pub async fn plan(&self, peer_id: &str, folder_id: &str) -> Result<SyncPlan> {
        let folder = find_folder(&self.config, folder_id)?;
        let peer = find_peer(&self.config, peer_id)?;
        let local = manifest::scan(folder)?;
        let remote: Manifest = self
            .get_json(
                peer,
                &format!("/v1/manifest?folder_id={}", encode_component(folder_id)),
            )
            .await?;
        if remote.folder_id != folder_id {
            bail!("peer returned a manifest for the wrong folder");
        }
        let base = self.state.get_base(folder_id, peer_id);
        let (decision, reason) = decide(
            &local.root_hash,
            local.files.is_empty(),
            &remote.root_hash,
            remote.files.is_empty(),
            base.as_deref(),
        );
        Ok(SyncPlan {
            folder_id: folder_id.to_owned(),
            peer_id: peer_id.to_owned(),
            local_hash: local.root_hash,
            remote_hash: remote.root_hash,
            base_hash: base,
            decision,
            reason,
        })
    }

    pub async fn sync(
        &self,
        peer_id: &str,
        folder_id: &str,
        action: SyncAction,
        accept_conflict: bool,
    ) -> Result<ApplyResult> {
        let _operation_lock = crate::operation_lock::OperationLock::acquire(&self.config.data_dir)?;
        let plan = self.plan(peer_id, folder_id).await?;
        let effective = match action {
            SyncAction::Auto => match plan.decision {
                PlanDecision::InSync => {
                    return Ok(ApplyResult {
                        folder_id: folder_id.to_owned(),
                        root_hash: plan.local_hash,
                        backup_version: None,
                    });
                }
                PlanDecision::Push => SyncAction::Push,
                PlanDecision::Pull => SyncAction::Pull,
                PlanDecision::Conflict => {
                    bail!(
                        "conflict: both sides differ from the last synced version; choose push or pull and explicitly accept the conflict"
                    )
                }
            },
            explicit => explicit,
        };
        let reverses_recommended_direction = matches!(
            (action, &plan.decision),
            (SyncAction::Push, PlanDecision::Pull) | (SyncAction::Pull, PlanDecision::Push)
        );
        if (plan.decision == PlanDecision::Conflict || reverses_recommended_direction)
            && !accept_conflict
        {
            bail!(
                "refusing to overwrite the changed side; repeat with --accept-conflict after reviewing the versions"
            );
        }

        match effective {
            SyncAction::Push => self.push(peer_id, folder_id, &plan).await,
            SyncAction::Pull => self.pull(peer_id, folder_id, &plan).await,
            SyncAction::Auto => unreachable!(),
        }
    }

    async fn push(&self, peer_id: &str, folder_id: &str, plan: &SyncPlan) -> Result<ApplyResult> {
        let peer = find_peer(&self.config, peer_id)?;
        let folder = find_folder(&self.config, folder_id)?;
        let prepared =
            archive::prepare_archive(folder, Some(&plan.local_hash), &self.config.data_dir)?;
        if prepared.manifest.root_hash != plan.local_hash {
            bail!("prepared snapshot does not match the sync plan");
        }
        let file = tokio::fs::File::open(prepared.file.path()).await?;
        let stream = ReaderStream::new(file);
        let url = format!(
            "{}/v1/apply?folder_id={}&expected_current={}&source_hash={}&source_device={}",
            peer.url.trim_end_matches('/'),
            encode_component(folder_id),
            encode_component(&plan.remote_hash),
            encode_component(&plan.local_hash),
            encode_component(&self.config.device.id),
        );
        let response = self
            .client
            .post(url)
            .bearer_auth(&peer.token)
            .header("content-type", "application/gzip")
            .body(Body::wrap_stream(stream))
            .send()
            .await?;
        let result: ApplyResult = parse_json_response(response).await?;
        if result.root_hash != plan.local_hash {
            bail!("peer acknowledged an unexpected root hash");
        }
        self.state.set_base(folder_id, peer_id, &plan.local_hash)?;
        Ok(result)
    }

    async fn pull(&self, peer_id: &str, folder_id: &str, plan: &SyncPlan) -> Result<ApplyResult> {
        let peer = find_peer(&self.config, peer_id)?;
        let folder = find_folder(&self.config, folder_id)?;
        let url = format!(
            "{}/v1/archive?folder_id={}&expected={}",
            peer.url.trim_end_matches('/'),
            encode_component(folder_id),
            encode_component(&plan.remote_hash)
        );
        let mut response = self.client.get(url).bearer_auth(&peer.token).send().await?;
        if !response.status().is_success() {
            return Err(response_error(response).await);
        }

        let temp = NamedTempFile::new_in(&self.config.data_dir)?;
        let mut output = tokio::fs::File::create(temp.path()).await?;
        while let Some(chunk) = response.chunk().await? {
            tokio::io::AsyncWriteExt::write_all(&mut output, &chunk).await?;
        }
        tokio::io::AsyncWriteExt::flush(&mut output).await?;
        drop(output);

        let result = archive::apply_archive(
            folder,
            temp.path(),
            &plan.remote_hash,
            Some(&plan.local_hash),
            &self.config.data_dir,
            self.config.history_limit,
        )?;
        self.state.set_base(folder_id, peer_id, &plan.remote_hash)?;
        let ack = AckRequest {
            folder_id: folder_id.to_owned(),
            peer_id: self.config.device.id.clone(),
            root_hash: plan.remote_hash.clone(),
        };
        let _: serde_json::Value = self.post_json(peer, "/v1/ack", &ack).await?;
        Ok(result)
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        peer: &crate::model::PeerConfig,
        path: &str,
    ) -> Result<T> {
        let response = self
            .client
            .get(format!("{}{}", peer.url.trim_end_matches('/'), path))
            .bearer_auth(&peer.token)
            .send()
            .await?;
        parse_json_response(response).await
    }

    async fn post_json<T: serde::de::DeserializeOwned>(
        &self,
        peer: &crate::model::PeerConfig,
        path: &str,
        body: &impl Serialize,
    ) -> Result<T> {
        let response = self
            .client
            .post(format!("{}{}", peer.url.trim_end_matches('/'), path))
            .bearer_auth(&peer.token)
            .json(body)
            .send()
            .await?;
        parse_json_response(response).await
    }
}

fn decide(
    local: &str,
    local_empty: bool,
    remote: &str,
    remote_empty: bool,
    base: Option<&str>,
) -> (PlanDecision, String) {
    if local == remote {
        return (
            PlanDecision::InSync,
            "both sides have identical content".into(),
        );
    }
    if base.is_none() && local_empty && !remote_empty {
        return (
            PlanDecision::Pull,
            "this folder is empty and the peer has initial content".into(),
        );
    }
    if base.is_none() && remote_empty && !local_empty {
        return (
            PlanDecision::Push,
            "the peer folder is empty and this device has initial content".into(),
        );
    }
    match base {
        Some(base) if local == base && remote != base => (
            PlanDecision::Pull,
            "only the peer changed since the last successful sync".into(),
        ),
        Some(base) if remote == base && local != base => (
            PlanDecision::Push,
            "only this device changed since the last successful sync".into(),
        ),
        Some(_) => (
            PlanDecision::Conflict,
            "both sides changed since the last successful sync".into(),
        ),
        None => (
            PlanDecision::Conflict,
            "no common sync baseline exists yet".into(),
        ),
    }
}

async fn parse_json_response<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T> {
    if !response.status().is_success() {
        return Err(response_error(response).await);
    }
    Ok(response.json().await?)
}

async fn response_error(response: reqwest::Response) -> anyhow::Error {
    let status = response.status();
    let text = response
        .text()
        .await
        .unwrap_or_else(|_| "<unreadable response>".into());
    anyhow::anyhow!("peer request failed with {status}: {text}")
}

fn encode_component(value: &str) -> String {
    // IDs and hashes have already been restricted to this safe subset.
    value.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decisions_require_a_common_baseline() {
        assert_eq!(
            decide("a", false, "b", false, None).0,
            PlanDecision::Conflict
        );
        assert_eq!(decide("a", false, "a", false, None).0, PlanDecision::InSync);
        assert_eq!(
            decide("a", false, "b", false, Some("a")).0,
            PlanDecision::Pull
        );
        assert_eq!(
            decide("a", false, "b", false, Some("b")).0,
            PlanDecision::Push
        );
        assert_eq!(
            decide("a", false, "b", false, Some("c")).0,
            PlanDecision::Conflict
        );
        assert_eq!(
            decide("empty", true, "save", false, None).0,
            PlanDecision::Pull
        );
        assert_eq!(
            decide("save", false, "empty", true, None).0,
            PlanDecision::Push
        );
    }
}
