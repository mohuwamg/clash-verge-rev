//! Recovery is restricted to the current IP's main/backup routes.
use super::policy::paired_candidates;
use anyhow::Result;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeOutcome {
    Healthy,
    Failed,
    VerificationRequired,
}

impl ProbeOutcome {
    pub fn aggregate(results: impl IntoIterator<Item = Self>) -> Self {
        let mut outcome = None;
        for result in results {
            if result == Self::Failed {
                return Self::Failed;
            }
            if outcome.is_none() || result == Self::VerificationRequired {
                outcome = Some(result);
            }
        }
        outcome.unwrap_or(Self::Failed)
    }
}

pub struct Snapshot {
    pub nodes: Vec<String>,
    pub current: Option<String>,
    pub current_result: ProbeOutcome,
}

#[async_trait::async_trait]
pub trait Backend: Send + Sync {
    async fn snapshot(&self) -> Result<Snapshot>;
    async fn check_active(&self) -> Result<()>;
    async fn switch_and_probe(&self, node: &str) -> Result<ProbeOutcome>;
    async fn accept(&self, node: &str) -> Result<()>;
    async fn wait_for_verification(&self, node: &str) -> Result<()>;
    async fn wait_to_retry(&self) -> Result<()>;
}

// Pending browser verification keeps the selected route without churning nodes.
async fn settle(backend: &impl Backend, node: &str, outcome: ProbeOutcome) -> Result<bool> {
    backend.check_active().await?;
    match outcome {
        ProbeOutcome::Healthy => backend.accept(node).await?,
        ProbeOutcome::VerificationRequired => backend.wait_for_verification(node).await?,
        ProbeOutcome::Failed => return Ok(false),
    }
    Ok(true)
}

/// Try only the opposite route on the current IP. The scheduler supplies a
/// 30-second retry after exhaustion; no subscription refresh or historical restore.
pub async fn run_round(backend: &impl Backend) -> Result<()> {
    let snapshot = backend.snapshot().await?;
    backend.check_active().await?;
    if let Some(node) = &snapshot.current
        && settle(backend, node, snapshot.current_result).await?
    {
        return Ok(());
    }
    for candidate in paired_candidates(&snapshot.nodes, snapshot.current.as_deref()) {
        backend.check_active().await?;
        let outcome = backend.switch_and_probe(&candidate).await?;
        if settle(backend, &candidate, outcome).await? {
            return Ok(());
        }
    }
    backend.check_active().await?;
    backend.wait_to_retry().await
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use anyhow::ensure;
    use std::sync::Mutex;

    const PRIMARY: &str = "美国 RCN 198.29.100.246 · 主线";
    const BACKUP: &str = "美国 RCN 198.29.100.246 · 备用";
    const OTHER: &str = "美国 Grande&RCN 66.167.8.36 · 主线";
    const FORMER_LAST_PRIMARY: &str = "美国 Grande&RCN 66.167.11.61 · 主线";
    const FORMER_LAST_BACKUP: &str = "美国 Grande&RCN 66.167.11.61 · 备用";

    struct State {
        calls: Vec<String>,
        nodes: Vec<String>,
        current: Option<String>,
        healthy: Option<String>,
        verification: Option<String>,
        cancel_after: Option<String>,
    }
    impl State {
        fn outcome(&self, node: Option<&str>) -> ProbeOutcome {
            if node.is_some() && node == self.healthy.as_deref() {
                ProbeOutcome::Healthy
            } else if node.is_some() && node == self.verification.as_deref() {
                ProbeOutcome::VerificationRequired
            } else {
                ProbeOutcome::Failed
            }
        }
    }
    struct Fake(Mutex<State>);
    impl Fake {
        fn new(current: Option<&str>, healthy: Option<&str>) -> Self {
            Self(Mutex::new(State {
                calls: Vec::new(),
                nodes: [OTHER, FORMER_LAST_PRIMARY, PRIMARY, FORMER_LAST_BACKUP, BACKUP]
                    .map(str::to_owned)
                    .into(),
                current: current.map(str::to_owned),
                healthy: healthy.map(str::to_owned),
                verification: None,
                cancel_after: None,
            }))
        }
        fn calls(&self) -> Vec<String> {
            self.0.lock().unwrap().calls.clone()
        }
    }
    #[async_trait::async_trait]
    impl Backend for Fake {
        async fn snapshot(&self) -> Result<Snapshot> {
            let mut state = self.0.lock().unwrap();
            state.calls.push("snapshot".into());
            Ok(Snapshot {
                nodes: state.nodes.clone(),
                current: state.current.clone(),
                current_result: state.outcome(state.current.as_deref()),
            })
        }
        async fn check_active(&self) -> Result<()> {
            let state = self.0.lock().unwrap();
            ensure!(
                state.cancel_after.is_none() || state.cancel_after != state.current,
                "cancelled"
            );
            Ok(())
        }
        async fn switch_and_probe(&self, node: &str) -> Result<ProbeOutcome> {
            let mut state = self.0.lock().unwrap();
            state.calls.push(format!("switch:{node}"));
            state.calls.push(format!("probe:{node}"));
            state.current = Some(node.into());
            Ok(state.outcome(Some(node)))
        }
        async fn accept(&self, node: &str) -> Result<()> {
            self.0.lock().unwrap().calls.push(format!("accept:{node}"));
            Ok(())
        }
        async fn wait_for_verification(&self, node: &str) -> Result<()> {
            self.0.lock().unwrap().calls.push(format!("verification:{node}"));
            Ok(())
        }
        async fn wait_to_retry(&self) -> Result<()> {
            self.0.lock().unwrap().calls.push("wait".into());
            Ok(())
        }
    }

    #[test]
    fn definitive_failure_overrides_verification_in_either_order() {
        use ProbeOutcome::*;
        assert_eq!(
            ProbeOutcome::aggregate([Healthy, VerificationRequired]),
            VerificationRequired
        );
        assert_eq!(
            ProbeOutcome::aggregate([VerificationRequired, Healthy]),
            VerificationRequired
        );
        assert_eq!(ProbeOutcome::aggregate([VerificationRequired, Failed]), Failed);
        assert_eq!(ProbeOutcome::aggregate([Failed, VerificationRequired]), Failed);
        assert_eq!(ProbeOutcome::aggregate([Healthy, Healthy]), Healthy);
        assert_eq!(ProbeOutcome::aggregate([]), Failed);
    }

    #[tokio::test]
    async fn switches_only_to_same_ip_opposite_route_in_both_directions() {
        for (current, counterpart) in [
            (PRIMARY, BACKUP),
            (BACKUP, PRIMARY),
            (FORMER_LAST_PRIMARY, FORMER_LAST_BACKUP),
            (FORMER_LAST_BACKUP, FORMER_LAST_PRIMARY),
        ] {
            let backend = Fake::new(Some(current), Some(counterpart));
            run_round(&backend).await.unwrap();
            assert_eq!(
                backend.calls(),
                [
                    "snapshot".into(),
                    format!("switch:{counterpart}"),
                    format!("probe:{counterpart}"),
                    format!("accept:{counterpart}")
                ]
            );
        }
    }

    #[tokio::test]
    async fn both_routes_failed_waits_without_using_a_healthy_other_ip() {
        let backend = Fake::new(Some(PRIMARY), Some(OTHER));
        run_round(&backend).await.unwrap();
        assert_eq!(
            backend.calls(),
            [
                "snapshot".into(),
                format!("switch:{BACKUP}"),
                format!("probe:{BACKUP}"),
                "wait".into()
            ]
        );
        // Recheck the actual target next round, regardless of cached latency.
        backend.0.lock().unwrap().healthy = Some(PRIMARY.into());
        run_round(&backend).await.unwrap();
        assert_eq!(
            &backend.calls()[4..],
            [
                "snapshot".into(),
                format!("switch:{PRIMARY}"),
                format!("probe:{PRIMARY}"),
                format!("accept:{PRIMARY}")
            ]
        );
        run_round(&backend).await.unwrap();
        assert_eq!(&backend.calls()[8..], ["snapshot".into(), format!("accept:{PRIMARY}")]);
    }

    #[tokio::test]
    async fn persistent_failure_yields_once_per_round_without_crossing_ip() {
        let backend = Fake::new(Some(PRIMARY), None);
        for _ in 0..3 {
            run_round(&backend).await.unwrap();
        }
        let calls = backend.calls();
        assert_eq!(calls.iter().filter(|call| *call == "wait").count(), 3);
        let switched: Vec<_> = calls.iter().filter_map(|call| call.strip_prefix("switch:")).collect();
        assert_eq!(switched, [BACKUP, PRIMARY, BACKUP]);
    }

    #[tokio::test]
    async fn healthy_current_is_kept_for_every_ip() {
        for current in [PRIMARY, BACKUP, OTHER, FORMER_LAST_PRIMARY, FORMER_LAST_BACKUP] {
            let backend = Fake::new(Some(current), Some(current));
            run_round(&backend).await.unwrap();
            assert_eq!(backend.calls(), ["snapshot".into(), format!("accept:{current}")]);
        }
    }

    #[tokio::test]
    async fn absent_or_unrecognized_current_and_missing_counterpart_never_pick_other_ips() {
        for current in [None, Some("unknown"), Some("US 1.2.3.4 · 主线"), Some(OTHER)] {
            let backend = Fake::new(current, Some(BACKUP));
            run_round(&backend).await.unwrap();
            assert_eq!(backend.calls(), ["snapshot", "wait"]);
        }
    }

    #[tokio::test]
    async fn verification_keeps_current_or_counterpart_and_recovers_next_round() {
        for node in [PRIMARY, BACKUP] {
            let backend = Fake::new(Some(PRIMARY), None);
            backend.0.lock().unwrap().verification = Some(node.into());
            run_round(&backend).await.unwrap();
            assert_eq!(backend.calls().last(), Some(&format!("verification:{node}")));
            assert!(!backend.calls().contains(&"wait".into()));
            let start = backend.calls().len();
            backend.0.lock().unwrap().healthy = Some(node.into());
            run_round(&backend).await.unwrap();
            assert_eq!(&backend.calls()[start..], ["snapshot".into(), format!("accept:{node}")]);
        }
    }

    #[tokio::test]
    async fn cancellation_stops_recovery_after_user_changes_selection() {
        let backend = Fake::new(Some(PRIMARY), Some(BACKUP));
        backend.0.lock().unwrap().cancel_after = Some(BACKUP.into());
        assert!(run_round(&backend).await.is_err());
        assert_eq!(
            backend.calls(),
            ["snapshot".into(), format!("switch:{BACKUP}"), format!("probe:{BACKUP}")]
        );
    }
}
