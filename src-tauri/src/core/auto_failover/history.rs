//! A bounded journal of confirmed changes to the managed node, independent of probe outcomes.
use anyhow::Result;
use clash_verge_logging::{Type, logging};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::{
    fs,
    io::AsyncWriteExt as _,
    sync::{Mutex, OnceCell},
};

const LIMIT: usize = 200;
static STORE: OnceCell<Mutex<History>> = OnceCell::const_new();

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SwitchReason {
    Automatic,
    Manual,
    Restore,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SwitchRecord {
    pub id: String,
    pub switched_at: u64,
    pub from_node: Option<String>,
    pub to_node: String,
    pub reason: SwitchReason,
}

struct History {
    path: Option<PathBuf>,
    entries: Vec<SwitchRecord>,
    error: Option<String>,
}

impl History {
    async fn load(path: PathBuf) -> Self {
        let result: Result<Vec<SwitchRecord>> = async {
            match fs::read(&path).await {
                Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
                Err(error) => Err(error.into()),
            }
        }
        .await;
        match result {
            Ok(mut entries) => {
                entries.truncate(LIMIT);
                Self {
                    path: Some(path),
                    entries,
                    error: None,
                }
            }
            // Preserve an unreadable/corrupt journal instead of silently overwriting it.
            Err(error) => Self {
                path: None,
                entries: Vec::new(),
                error: Some(error.to_string()),
            },
        }
    }

    fn append(&mut self, from: Option<&str>, to: &str, reason: SwitchReason, at: u64) -> bool {
        if from == Some(to) {
            return false;
        }
        self.entries.insert(
            0,
            SwitchRecord {
                id: nanoid::nanoid!(),
                switched_at: at,
                from_node: from.map(str::to_owned),
                to_node: to.into(),
                reason,
            },
        );
        self.entries.truncate(LIMIT);
        true
    }

    async fn save(&self) -> Result<()> {
        let path = self
            .path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("history_unavailable"))?;
        let temporary = path.with_extension("json.tmp");
        let bytes = serde_json::to_vec(&self.entries)?;
        let mut file = fs::File::create(&temporary).await?;
        file.write_all(&bytes).await?;
        file.sync_all().await?;
        drop(file);
        fs::rename(&temporary, path).await?;
        Ok(())
    }

    fn publish(&self) {
        super::publish(|state| {
            state.switch_history.clone_from(&self.entries);
            state.switch_history_error = self.error.is_some();
        });
    }
}

async fn store() -> &'static Mutex<History> {
    STORE
        .get_or_init(|| async {
            let history = match crate::utils::dirs::app_home_dir() {
                Ok(root) => History::load(root.join("auto-failover-switch-history.json")).await,
                Err(error) => History {
                    path: None,
                    entries: Vec::new(),
                    error: Some(error.to_string()),
                },
            };
            if let Some(error) = &history.error {
                logging!(warn, Type::Core, "Auto failover switch history could not load: {error}");
            }
            Mutex::new(history)
        })
        .await
}

pub async fn initialize() {
    store().await.lock().await.publish();
}

/// The caller has already read the selected node back from the core.
/// Journal failures must not turn a successful node change into a failed switch.
pub async fn record(from: Option<&str>, to: &str, reason: SwitchReason) {
    let at = super::now();
    let mut history = store().await.lock().await;
    if !history.append(from, to, reason, at) {
        return;
    }
    logging!(
        info,
        Type::Core,
        "Auto failover node switched: at={}, from={:?}, to={}, reason={:?}",
        at,
        from,
        to,
        reason
    );
    if history.path.is_some() {
        history.error = history.save().await.err().map(|error| error.to_string());
        if let Some(error) = &history.error {
            logging!(warn, Type::Core, "Auto failover switch history could not save: {error}");
        }
    }
    history.publish();
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn journal() -> History {
        History {
            path: None,
            entries: Vec::new(),
            error: None,
        }
    }

    #[test]
    fn same_node_is_not_a_switch_and_history_is_newest_first_and_bounded() {
        let mut history = journal();
        assert!(!history.append(Some("a"), "a", SwitchReason::Manual, 1));
        assert!(history.entries.is_empty());
        for index in 0..205 {
            history.append(Some("a"), "b", SwitchReason::Automatic, index);
        }
        assert_eq!(history.entries.len(), LIMIT);
        assert_eq!(history.entries[0].switched_at, 204);
        assert_eq!(history.entries[LIMIT - 1].switched_at, 5);
    }

    #[tokio::test]
    async fn journal_survives_restart_and_corrupt_files_are_preserved() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../.zkit/tmp/switch-history-{unique}"));
        fs::create_dir_all(&dir).await.unwrap();
        scopeguard::defer! { let _ = std::fs::remove_dir_all(&dir); }
        let path = dir.join("history.json");
        let mut history = History::load(path.clone()).await;
        history.append(Some("节点 A"), "节点 B", SwitchReason::Automatic, 123);
        history.save().await.unwrap();
        history.append(Some("节点 B"), "节点 A", SwitchReason::Restore, 456);
        history.save().await.unwrap();
        assert_eq!(History::load(path.clone()).await.entries, history.entries);
        fs::write(&path, b"invalid journal").await.unwrap();
        let mut broken = History::load(path.clone()).await;
        assert!(broken.error.is_some());
        broken.append(None, "节点 C", SwitchReason::Manual, 789);
        assert!(broken.save().await.is_err());
        assert_eq!(fs::read(path).await.unwrap(), b"invalid journal");
        assert_eq!(broken.entries[0].to_node, "节点 C");
    }
}
