mod history;
mod http_probe;
pub mod policy;
mod recovery;

use self::policy::{AutoFailoverConfig, GROUP, INTERVAL_SECONDS, next_check_delay};
use crate::{
    config::{Config, IVerge, PrfItem, PrfOption, profiles},
    core::{CoreManager, handle::Handle, tray::Tray},
    process::AsyncHandler,
};
use anyhow::{Context as _, Result, bail, ensure};
use clash_verge_logging::{Type, logging};
use futures::{FutureExt as _, StreamExt as _, stream};
use parking_lot::RwLock;
use serde::Serialize;
use std::{
    future::Future,
    sync::{
        LazyLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, Notify};

static STARTED: AtomicBool = AtomicBool::new(false);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static MEASURED_GENERATION: AtomicU64 = AtomicU64::new(u64::MAX);
static NODESET_REVISION: AtomicU64 = AtomicU64::new(0);
static CLEANUP_NEEDED: AtomicBool = AtomicBool::new(false);
static EXTERNAL_UPDATE: AtomicBool = AtomicBool::new(false);
static WAKE: Notify = Notify::const_new();
static MEASURE_WAKE: Notify = Notify::const_new();
static MUTATION: Mutex<()> = Mutex::const_new(());
static MEASUREMENT: Mutex<()> = Mutex::const_new(());
static STATE: LazyLock<RwLock<FailoverStatus>> = LazyLock::new(|| RwLock::new(FailoverStatus::default()));
/// Shared by automatic, scheduled and manual subscription refreshes.
pub(crate) static SUBSCRIPTION_UPDATE: Mutex<()> = Mutex::const_new(());

#[derive(Clone, Default, Serialize)]
pub struct NodeHealth {
    pub name: String,
    pub delay_ms: Option<u32>,
    pub checked_at: u64,
    pub error: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct WebsiteHealth {
    pub url: String,
    pub node: String,
    pub reachable: bool,
    pub verification_required: bool,
    pub status_code: Option<u16>,
    pub elapsed_ms: u64,
    pub checked_at: u64,
    pub error: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct FailoverStatus {
    pub phase: String,
    pub message: Option<String>,
    pub current_node: Option<String>,
    pub last_healthy_node: Option<String>,
    pub nodes: Vec<NodeHealth>,
    pub websites: Vec<WebsiteHealth>,
    pub switch_history: Vec<history::SwitchRecord>,
    pub switch_history_error: bool,
    pub checked_at: Option<u64>,
    pub next_check_at: Option<u64>,
    pub subscription_updated_at: Option<u64>,
    pub measuring: bool,
    pub generation: u64,
}

impl Default for FailoverStatus {
    fn default() -> Self {
        Self {
            phase: "disabled".into(),
            message: None,
            current_node: None,
            last_healthy_node: None,
            nodes: Vec::new(),
            websites: Vec::new(),
            switch_history: Vec::new(),
            switch_history_error: false,
            checked_at: None,
            next_check_at: None,
            subscription_updated_at: None,
            measuring: false,
            generation: 0,
        }
    }
}

impl FailoverStatus {
    fn reset_runtime(&mut self) {
        let history = std::mem::take(&mut self.switch_history);
        let history_error = self.switch_history_error;
        *self = Self::default();
        self.switch_history = history;
        self.switch_history_error = history_error;
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn status() -> FailoverStatus {
    STATE.read().clone()
}

fn publish(change: impl FnOnce(&mut FailoverStatus)) {
    let snapshot = {
        let mut state = STATE.write();
        change(&mut state);
        state.clone()
    };
    Handle::notify_auto_failover(&snapshot);
}

pub fn invalidate() {
    GENERATION.fetch_add(1, Ordering::AcqRel);
}

pub fn wake() {
    WAKE.notify_one();
    MEASURE_WAKE.notify_one();
}

pub fn request_check() {
    invalidate();
    wake();
}

pub async fn settings() -> AutoFailoverConfig {
    Config::verge()
        .await
        .latest_arc()
        .auto_failover
        .clone()
        .unwrap_or_default()
}

#[derive(Clone)]
struct Context {
    config: AutoFailoverConfig,
    generation: u64,
    uid: String,
}

impl Context {
    async fn current() -> Result<Self> {
        let generation = GENERATION.load(Ordering::Acquire);
        let config = settings().await;
        ensure!(config.enabled, "disabled");
        config.validate()?;
        let uid = config.profile_uid.clone().context("subscription_not_bound")?;
        let context = Self {
            config,
            generation,
            uid,
        };
        context.check().await?;
        Ok(context)
    }

    async fn check(&self) -> Result<()> {
        ensure!(!Handle::global().is_exiting(), "application_exiting");
        ensure!(self.generation == GENERATION.load(Ordering::Acquire), "cancelled");
        ensure!(!EXTERNAL_UPDATE.load(Ordering::Acquire), "subscription_updating");
        ensure!(settings().await == self.config, "configuration_changed");
        let profiles = Config::profiles().await.latest_arc();
        ensure!(
            profiles.current.as_deref() == Some(self.uid.as_str()),
            "profile_mismatch"
        );
        let current = profiles.get_item(&self.uid)?;
        ensure!(
            current.url.as_deref() == Some(self.config.subscription_url.as_str()),
            "profile_mismatch"
        );
        Ok(())
    }

    // Cancel only read-only network work. Mutations always finish under MUTATION before settings change.
    async fn read<T>(&self, future: impl Future<Output = Result<T>>) -> Result<T> {
        tokio::select! {
            result = future => { self.check().await?; result },
            () = async {
                loop {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    if self.check().await.is_err() { break; }
                }
            } => bail!("cancelled"),
        }
    }

    async fn phase(&self, phase: &str, message: Option<String>) -> Result<()> {
        self.check().await?;
        publish(|state| {
            state.phase = phase.into();
            state.next_check_at = None;
            state.message = message;
            state.generation = self.generation;
        });
        Ok(())
    }
}

pub fn start() {
    if STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    AsyncHandler::spawn(|| async {
        history::initialize().await;
        tokio::join!(monitor_loop(), measurement_loop());
    });
}

async fn monitor_loop() {
    let mut timer = tokio::time::interval(Duration::from_secs(INTERVAL_SECONDS));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! { _ = timer.tick() => {}, () = WAKE.notified() => {} }
        if Handle::global().is_exiting() {
            return;
        }
        let began = Instant::now();
        match Context::current().await {
            Ok(context) => {
                if let Err(error) = monitor(&context).await
                    && context.check().await.is_ok()
                {
                    let _ = context
                        .phase("retry_wait", Some(crate::utils::help::mask_err(&error.to_string())))
                        .await;
                }
                if context.check().await.is_ok() {
                    let state = status();
                    let delay = next_check_delay(&state.phase, state.message.as_deref(), began.elapsed());
                    timer.reset_after(delay);
                    publish(|state| state.next_check_at = Some(now() + delay.as_millis() as u64));
                } else {
                    wake();
                }
            }
            Err(error) => {
                let enabled = settings().await.enabled;
                if !enabled && CLEANUP_NEEDED.load(Ordering::Acquire) {
                    let _mutation = MUTATION.lock().await;
                    if !settings().await.enabled {
                        let _ = release_global_route().await;
                        if CoreManager::global().update_config_checked().await.is_ok() {
                            CLEANUP_NEEDED.store(false, Ordering::Release);
                        }
                    }
                }
                publish(|state| {
                    state.phase = if enabled { "paused" } else { "disabled" }.into();
                    state.message = if enabled {
                        Some(error.to_string())
                    } else {
                        CLEANUP_NEEDED
                            .load(Ordering::Acquire)
                            .then(|| "routing_cleanup_pending".into())
                    };
                    state.next_check_at = None;
                    state.measuring = false;
                });
            }
        }
    }
}

async fn measurement_loop() {
    let mut timer = tokio::time::interval(Duration::from_secs(INTERVAL_SECONDS));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! { _ = timer.tick() => {}, () = MEASURE_WAKE.notified() => {} }
        if Handle::global().is_exiting() {
            return;
        }
        if let Ok(context) = Context::current().await {
            let _ = measure(&context, false).await;
        }
    }
}

async fn nodes(context: &Context) -> Result<(Vec<String>, Option<String>)> {
    context
        .read(async {
            let proxies = tokio::time::timeout(Duration::from_secs(5), Handle::mihomo().get_proxies()).await??;
            let group = proxies.proxies.get(GROUP).context("managed_group_not_loaded")?;
            let mut names = Vec::new();
            for name in group.all.as_deref().unwrap_or_default() {
                if let Some(proxy) = proxies.proxies.get(name) {
                    let kind = serde_json::to_value(&proxy.proxy_type)?;
                    if proxy.all.is_none()
                        && !matches!(
                            kind.as_str(),
                            Some("Direct" | "Reject" | "RejectDrop" | "Pass" | "PassRule" | "Dns" | "Compatible")
                        )
                        && !names.contains(name)
                    {
                        names.push(name.clone());
                    }
                }
            }
            Ok((names, group.now.clone()))
        })
        .await
}

async fn measure(context: &Context, force: bool) -> Result<Vec<NodeHealth>> {
    let _measurement = MEASUREMENT.lock().await;
    context.check().await?;
    let snapshot = status();
    if !force
        && MEASURED_GENERATION.load(Ordering::Acquire) == context.generation
        && !snapshot.nodes.is_empty()
        && snapshot
            .nodes
            .iter()
            .all(|node| now().saturating_sub(node.checked_at) < 55_000)
    {
        return Ok(snapshot.nodes);
    }
    let revision = NODESET_REVISION.load(Ordering::Acquire);
    let (names, _) = nodes(context).await?;
    publish(|state| state.measuring = true);
    let jobs: Vec<_> = names
        .into_iter()
        .map(|name| {
            let config = context.config.clone();
            async move {
                let mut error = None;
                let mut delay_ms = None;
                for url in &config.delay_urls {
                    let response = tokio::time::timeout(
                        Duration::from_millis(u64::from(config.delay_timeout_ms) + 1_000),
                        Handle::mihomo().delay_proxy_by_name(&name, url, config.delay_timeout_ms),
                    )
                    .await;
                    match response {
                        Ok(Ok(delay)) if delay.delay > 0 => {
                            delay_ms = Some(delay.delay);
                            error = None;
                            break;
                        }
                        Ok(Err(_)) => error = Some("delay_request_failed".into()),
                        _ => error = Some("delay_timeout".into()),
                    }
                }
                NodeHealth {
                    name,
                    delay_ms,
                    checked_at: now(),
                    error,
                }
            }
            .boxed()
        })
        .collect();
    let concurrency = context.config.concurrency;
    let results = context
        .read(async move { Ok(stream::iter(jobs).buffered(concurrency).collect::<Vec<_>>().await) })
        .await;
    let mut revision_current = false;
    if context.check().await.is_ok() {
        publish(|state| {
            state.measuring = false;
            // Check under the same state lock used by refresh to clear the cache.
            revision_current = revision == NODESET_REVISION.load(Ordering::Acquire);
            if revision_current && let Ok(nodes) = &results {
                state.nodes = nodes.clone();
                MEASURED_GENERATION.store(context.generation, Ordering::Release);
                state.generation = context.generation;
            }
        });
    }
    ensure!(revision_current, "subscription_changed_during_measurement");
    results
}

async fn ensure_route(context: &Context, expected_node: Option<&str>) -> Result<u16> {
    context
        .read(async {
            let base = tokio::time::timeout(Duration::from_secs(5), Handle::mihomo().get_base_config()).await??;
            match base.mode.to_string().as_str() {
                "rule" => {
                    let rules = tokio::time::timeout(Duration::from_secs(5), Handle::mihomo().get_rules()).await??;
                    for (rule, domain) in rules.rules.iter().zip(context.config.domains()) {
                        ensure!(
                            rule.proxy == GROUP
                                && rule.payload == domain
                                && serde_json::to_value(&rule.rule_type)?.as_str() == Some("DomainSuffix"),
                            "managed_rules_not_active"
                        );
                    }
                    ensure!(
                        rules.rules.len() >= context.config.domains().len(),
                        "managed_rules_missing"
                    );
                }
                "global" => {
                    let global =
                        tokio::time::timeout(Duration::from_secs(5), Handle::mihomo().get_proxy_by_name("GLOBAL"))
                            .await??;
                    ensure!(global.now.as_deref() == Some(GROUP), "global_group_not_selected");
                }
                _ => bail!("direct_mode: switch to Rule or Global mode"),
            }
            if let Some(expected) = expected_node {
                let selected =
                    tokio::time::timeout(Duration::from_secs(5), Handle::mihomo().get_proxy_by_name(GROUP)).await??;
                ensure!(selected.now.as_deref() == Some(expected), "selection_changed");
            }
            ensure!(base.mixed_port > 0, "mixed_proxy_port_unavailable");
            Ok(base.mixed_port)
        })
        .await
}

async fn activate_global_route(context: &Context) -> Result<()> {
    let _mutation = MUTATION.lock().await;
    context.check().await?;
    let base = tokio::time::timeout(Duration::from_secs(5), Handle::mihomo().get_base_config()).await??;
    if base.mode.to_string() == "global" {
        let current = Handle::mihomo().get_proxy_by_name("GLOBAL").await?;
        if current.now.as_deref() != Some(GROUP) {
            Handle::mihomo().select_node_for_group("GLOBAL", GROUP).await?;
            context.check().await?;
            profiles::record_selected_node("GLOBAL", GROUP).await?;
        }
    }
    Ok(())
}

// Read back core state before reporting a successful switch, including manual changes.
async fn confirm_selection(previous: Option<&str>, node: &str, reason: history::SwitchReason) -> Result<()> {
    let selected = tokio::time::timeout(Duration::from_secs(5), Handle::mihomo().get_proxy_by_name(GROUP)).await??;
    ensure!(selected.now.as_deref() == Some(node), "selection_not_applied");
    publish(|state| {
        state.current_node = Some(node.into());
        if previous != Some(node) {
            state.websites.clear();
        }
    });
    history::record(previous, node, reason).await;
    Ok(())
}

async fn select(context: &Context, node: &str, reason: history::SwitchReason) -> Result<()> {
    let _mutation = MUTATION.lock().await;
    let _profiles = profiles::PROFILE_WRITE_LOCK.lock().await;
    context.check().await?;
    context.phase("switching", Some(node.into())).await?;
    let previous = Handle::mihomo().get_proxy_by_name(GROUP).await?.now;
    profiles::supersede_selected_activation();
    tokio::time::timeout(
        Duration::from_secs(5),
        Handle::mihomo().select_node_for_group(GROUP, node),
    )
    .await??;
    confirm_selection(previous.as_deref(), node, reason).await?;
    context.check().await?;
    ensure_route(context, Some(node)).await?;
    if Config::verge()
        .await
        .latest_arc()
        .auto_close_connection
        .unwrap_or(false)
        && let Some(previous) = previous.filter(|previous| previous != node)
    {
        let _ = tokio::time::timeout(Duration::from_secs(5), async {
            let connections = Handle::mihomo().get_connections().await?;
            for connection in connections.connections.unwrap_or_default() {
                if connection.chains.iter().any(|chain| chain == GROUP) && connection.chains.contains(&previous) {
                    let _ = Handle::mihomo().close_connection(&connection.id).await;
                }
            }
            Ok::<(), anyhow::Error>(())
        })
        .await;
    }
    context.check().await?;
    publish(|state| {
        state.current_node = Some(node.into());
        state.websites.clear();
    });
    Handle::refresh_proxy_config();
    Ok(())
}

async fn probe(context: &Context, node: &str) -> Result<recovery::ProbeOutcome> {
    let port = ensure_route(context, Some(node)).await?;
    context.phase("checking", None).await?;
    let client = http_probe::client(port, context.config.website_timeout_ms, context.config.domains())?;
    let jobs: Vec<_> = context
        .config
        .websites
        .iter()
        .map(|url| {
            let client = client.clone();
            let url = url.clone();
            let node = node.to_owned();
            async move {
                let started = Instant::now();
                let (status_code, result) = http_probe::check(&client, &url).await;
                WebsiteHealth {
                    url,
                    node,
                    reachable: result.is_ok(),
                    verification_required: result
                        .as_ref()
                        .is_err_and(|error| error.to_string() == "verification_required"),
                    status_code,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    checked_at: now(),
                    error: result
                        .err()
                        .map(|error| crate::utils::help::mask_err(&error.to_string())),
                }
            }
            .boxed()
        })
        .collect();
    let concurrency = context.config.concurrency;
    let results = context
        .read(async move { Ok(stream::iter(jobs).buffered(concurrency).collect::<Vec<_>>().await) })
        .await?;
    ensure_route(context, Some(node)).await?;
    let outcome = recovery::ProbeOutcome::aggregate(results.iter().map(|result| {
        if result.reachable {
            recovery::ProbeOutcome::Healthy
        } else if result.verification_required {
            recovery::ProbeOutcome::VerificationRequired
        } else {
            recovery::ProbeOutcome::Failed
        }
    }));
    for result in &results {
        logging!(
            info,
            Type::Core,
            "Auto failover probe: node={}, website={}, reachable={}, status={:?}, error={:?}",
            node,
            result.url,
            result.reachable,
            result.status_code,
            result.error
        );
    }
    publish(|state| {
        state.websites = results;
        state.current_node = Some(node.into());
        state.checked_at = Some(now());
    });
    Ok(outcome)
}

async fn mark_healthy(context: &Context, node: &str) -> Result<()> {
    let _mutation = MUTATION.lock().await;
    let _profiles = profiles::PROFILE_WRITE_LOCK.lock().await;
    context.check().await?;
    ensure_route(context, Some(node)).await?;
    profiles::record_selected_node(GROUP, node).await?;
    publish(|state| {
        state.last_healthy_node = Some(node.into());
        state.phase = "healthy".into();
        state.message = None;
    });
    Handle::refresh_proxy_config();
    let _ = Tray::global().update_menu().await;
    Ok(())
}

struct LiveBackend<'a>(&'a Context);

#[async_trait::async_trait]
impl recovery::Backend for LiveBackend<'_> {
    async fn snapshot(&self) -> Result<recovery::Snapshot> {
        let context = self.0;
        activate_global_route(context).await?;
        let (names, current) = nodes(context).await?;
        // Recovery probes the actual target on each route. Display-only delay
        // measurements must not suppress a retry or keep a 55-second failure cache.
        let result = match current.as_deref().filter(|name| names.iter().any(|node| node == name)) {
            Some(node) => probe(context, node).await?,
            None => recovery::ProbeOutcome::Failed,
        };
        Ok(recovery::Snapshot {
            nodes: names,
            current,
            current_result: result,
        })
    }

    async fn check_active(&self) -> Result<()> {
        self.0.check().await
    }

    async fn switch_and_probe(&self, candidate: &str) -> Result<recovery::ProbeOutcome> {
        if let Err(error) = select(self.0, candidate, history::SwitchReason::Automatic).await {
            self.0.check().await?;
            self.0.phase("switching", Some(error.to_string())).await?;
            return Ok(recovery::ProbeOutcome::Failed);
        }
        probe(self.0, candidate).await
    }

    async fn accept(&self, node: &str) -> Result<()> {
        mark_healthy(self.0, node).await
    }

    async fn wait_for_verification(&self, node: &str) -> Result<()> {
        let _mutation = MUTATION.lock().await;
        let _profiles = profiles::PROFILE_WRITE_LOCK.lock().await;
        self.0.check().await?;
        ensure_route(self.0, Some(node)).await?;
        profiles::record_selected_node(GROUP, node).await?;
        publish(|state| {
            state.phase = "verification_required".into();
            state.message = Some("verification_required".into());
        });
        Handle::refresh_proxy_config();
        let _ = Tray::global().update_menu().await;
        Ok(())
    }

    async fn wait_to_retry(&self) -> Result<()> {
        self.0.phase("retry_wait", Some("pair_unavailable".into())).await
    }
}

async fn monitor(context: &Context) -> Result<()> {
    if context
        .read(async {
            tokio::time::timeout(Duration::from_secs(5), Handle::mihomo().get_version()).await??;
            Ok(())
        })
        .await
        .is_err()
    {
        context.phase("waiting_core", Some("core_unavailable".into())).await?;
        return Ok(());
    }
    context.phase("checking", None).await?;
    recovery::run_round(&LiveBackend(context)).await
}

pub async fn configure(mut config: AutoFailoverConfig) -> Result<()> {
    config.validate()?;
    invalidate();
    let _mutation = MUTATION.lock().await;
    scopeguard::defer!(wake());
    let old = settings().await;
    let previous_profile = Config::profiles().await.latest_arc().current.clone();
    // UID is backend-owned, never trust a supplied subscription binding.
    config.profile_uid = old.profile_uid.clone();
    if config.enabled {
        let profiles = Config::profiles().await.latest_arc();
        config.profile_uid = profiles
            .items
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|item| item.url.as_deref() == Some(config.subscription_url.as_str()))
            .and_then(|item| item.uid.as_ref().map(ToString::to_string));
        if config.profile_uid.is_none() {
            let option = PrfOption {
                timeout_seconds: Some(15),
                ..PrfOption::default()
            };
            let mut item = PrfItem::from_url(&config.subscription_url, None, None, Some(&option)).await?;
            profiles::profiles_append_item_safe(&mut item).await?;
            profiles::profiles_save_file_safe().await?;
            config.profile_uid = item.uid.as_ref().map(ToString::to_string);
            Handle::refresh_profiles();
        }
    }
    if old.enabled && !config.enabled {
        let _ = release_global_route().await;
    }
    let patch = IVerge {
        auto_failover: Some(config.clone()),
        ..IVerge::default()
    };
    crate::feat::patch_verge(&patch, false).await?;
    if !config.enabled {
        // Disabling must remain possible when the core is offline. Retry runtime cleanup later.
        let cleanup = CoreManager::global().update_config_checked().await;
        CLEANUP_NEEDED.store(cleanup.is_err(), Ordering::Release);
        publish(|state| {
            state.reset_runtime();
            state.message = cleanup.err().map(|_| "routing_cleanup_pending".into());
        });
        Handle::refresh_clash();
        start();
        return Ok(());
    }
    CLEANUP_NEEDED.store(false, Ordering::Release);
    let result: Result<()> = async {
        if config.enabled
            && let Some(uid) = &config.profile_uid
        {
            let outcome = crate::cmd::patch_profiles_config_by_profile_index(uid.clone().into())
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            ensure!(outcome.is_valid(), "subscription_activation_failed: {outcome}");
        }

        Ok(())
    }
    .await;
    if let Err(error) = result {
        let rollback = IVerge {
            auto_failover: Some(old),
            ..IVerge::default()
        };
        crate::feat::patch_verge(&rollback, false).await?;
        if let Some(uid) = previous_profile {
            let _ = crate::cmd::patch_profiles_config_by_profile_index(uid).await;
        }
        let _ = CoreManager::global().update_config_checked().await;
        return Err(error);
    }
    publish(FailoverStatus::reset_runtime);
    Handle::refresh_clash();
    start();
    wake();
    Ok(())
}

/// Handles user selection atomically with respect to recovery. Other groups use the existing path.
pub async fn manual_select(group: &str, node: &str) -> Result<bool> {
    if group != GROUP || !settings().await.enabled {
        return Ok(false);
    }
    invalidate();
    let _mutation = MUTATION.lock().await;
    scopeguard::defer!(wake());
    let _profiles = profiles::PROFILE_WRITE_LOCK.lock().await;
    if Context::current().await.is_err() {
        return Ok(false);
    }
    let previous = tokio::time::timeout(Duration::from_secs(5), Handle::mihomo().get_proxy_by_name(group))
        .await??
        .now;
    profiles::supersede_selected_activation();
    tokio::time::timeout(
        Duration::from_secs(5),
        Handle::mihomo().select_node_for_group(group, node),
    )
    .await??;
    confirm_selection(previous.as_deref(), node, history::SwitchReason::Manual).await?;
    profiles::record_selected_node(group, node).await?;
    Handle::refresh_proxy_config();
    Ok(true)
}

/// Retain the selected physical node when removing our group from a global-mode config.
async fn release_global_route() -> Result<()> {
    let Ok(global) = Handle::mihomo().get_proxy_by_name("GLOBAL").await else {
        return Ok(());
    };
    if global.now.as_deref() != Some(GROUP) {
        return Ok(());
    }
    let selected = Handle::mihomo().get_proxy_by_name(GROUP).await?;
    if let Some(node) = selected.now {
        Handle::mihomo().select_node_for_group("GLOBAL", &node).await?;
        profiles::record_selected_node("GLOBAL", &node).await?;
    }
    Ok(())
}

/// A normal scheduled/manual refresh invalidates probes until the new config has settled.
pub(crate) struct SubscriptionUpdatePause(bool);

impl Drop for SubscriptionUpdatePause {
    fn drop(&mut self) {
        if self.0 {
            NODESET_REVISION.fetch_add(1, Ordering::AcqRel);
            EXTERNAL_UPDATE.store(false, Ordering::Release);
            request_check();
        }
    }
}

pub(crate) async fn pause_for_subscription_update(uid: &str) -> SubscriptionUpdatePause {
    let config = settings().await;
    let managed = config.enabled && config.profile_uid.as_deref() == Some(uid);
    if managed {
        EXTERNAL_UPDATE.store(true, Ordering::Release);
        invalidate();
    }
    SubscriptionUpdatePause(managed)
}

#[cfg(test)]
mod switch_history_tests {
    use super::*;

    #[test]
    fn settings_reset_preserves_switch_history() {
        let record = history::SwitchRecord {
            id: "test-switch".into(),
            switched_at: 123,
            from_node: Some("a".into()),
            to_node: "b".into(),
            reason: history::SwitchReason::Automatic,
        };
        let mut state = FailoverStatus {
            switch_history: vec![record.clone()],
            switch_history_error: true,
            current_node: Some("b".into()),
            ..FailoverStatus::default()
        };
        state.reset_runtime();
        assert_eq!(state.switch_history, vec![record]);
        assert!(state.switch_history_error);
        assert!(state.current_node.is_none());
    }
}

/// Persist and apply a profile-bound entrance pool while excluding automatic
/// recovery. A failed validation or activation restores the previous settings.
pub async fn configure_entry_pool(mut pool: crate::config::entry_pool::EntryPoolConfig) -> Result<()> {
    invalidate();
    let _mutation = MUTATION.lock().await;
    scopeguard::defer!(wake());
    let old = Config::verge()
        .await
        .latest_arc()
        .entry_pool
        .clone()
        .unwrap_or_default();
    let current_uid = Config::profiles().await.latest_arc().current.clone();
    let failover = settings().await;
    if pool.enabled {
        let uid = current_uid.as_ref().context("请先选择订阅")?.to_string();
        pool.profile_uid = Some(uid.clone());
        ensure!(!pool.entries.is_empty(), "请选择入口节点");
        ensure!(!pool.exit_node.is_empty(), "请选择出口节点");
        if failover.enabled && failover.profile_uid.as_deref() == Some(uid.as_str()) {
            pool.target_group = GROUP.into();
        }
    }
    let target = pool.target_group.clone();
    let previous = if pool.enabled {
        Handle::mihomo()
            .get_proxy_by_name(&target)
            .await
            .ok()
            .and_then(|g| g.now)
    } else {
        None
    };
    let applied: Result<()> = async {
        crate::feat::patch_verge(
            &IVerge {
                entry_pool: Some(pool.clone()),
                ..IVerge::default()
            },
            false,
        )
        .await?;
        CoreManager::global().update_config_checked().await?;
        if pool.enabled {
            let _profiles = profiles::PROFILE_WRITE_LOCK.lock().await;
            ensure!(
                Config::profiles().await.latest_arc().current == current_uid,
                "订阅已变化，请重试"
            );
            profiles::supersede_selected_activation();
            Handle::mihomo().select_node_for_group(&target, &pool.exit_node).await?;
            let selected = Handle::mihomo().get_proxy_by_name(&target).await?;
            ensure!(
                selected.now.as_deref() == Some(pool.exit_node.as_str()),
                "出口选择未生效"
            );
            profiles::record_selected_node(&target, &pool.exit_node).await?;
        }
        Ok(())
    }
    .await;
    if let Err(error) = applied {
        crate::feat::patch_verge(
            &IVerge {
                entry_pool: Some(old),
                ..IVerge::default()
            },
            false,
        )
        .await?;
        let rollback = CoreManager::global().update_config_checked().await;
        if rollback.is_ok()
            && let Some(previous) = previous
        {
            let _ = Handle::mihomo().select_node_for_group(&target, &previous).await;
        }
        return Err(match rollback {
            Ok(()) => error,
            Err(rollback) => error.context(format!("恢复配置失败：{rollback}")),
        });
    }
    MEASURED_GENERATION.store(u64::MAX, Ordering::Release);
    Handle::refresh_clash();
    Handle::refresh_proxy_config();
    Ok(())
}
