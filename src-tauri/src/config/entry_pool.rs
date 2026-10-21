use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_yaml_ng::{Mapping, Value};
use std::collections::BTreeSet;

use crate::core::auto_failover::policy::{GROUP, paired_candidates};

pub const ENTRY_GROUP: &str = "ZK Chain Entry";

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct EntryPoolConfig {
    pub enabled: bool,
    pub profile_uid: Option<String>,
    pub entries: Vec<String>,
    pub exit_node: String,
    pub target_group: String,
}

impl EntryPoolConfig {
    pub fn exits(&self, names: &[String]) -> Vec<String> {
        let mut exits = vec![self.exit_node.clone()];
        exits.extend(paired_candidates(names, Some(&self.exit_node)));
        exits
    }

    pub fn validate(&self, config: &Mapping) -> Result<Vec<String>> {
        ensure!(
            !self.entries.is_empty() && self.entries.len() <= 50,
            "请选择 1–50 个入口节点"
        );
        ensure!(!self.exit_node.is_empty(), "请选择出口节点");
        ensure!(!self.target_group.is_empty(), "请选择目标代理组");
        let proxies = config
            .get("proxies")
            .and_then(Value::as_sequence)
            .ok_or_else(|| anyhow::anyhow!("当前配置没有独立节点，暂不支持代理集合中的节点"))?;
        let names: Vec<String> = proxies
            .iter()
            .filter_map(|p| p.get("name")?.as_str().map(str::to_owned))
            .collect();
        let exits = self.exits(&names);
        let mut selected = BTreeSet::new();
        for name in self.entries.iter().chain(&exits) {
            ensure!(selected.insert(name), "入口不能重复，也不能与出口主备重叠：{name}");
            let matching: Vec<_> = proxies
                .iter()
                .filter(|p| p.get("name").and_then(Value::as_str) == Some(name))
                .collect();
            ensure!(matching.len() == 1, "节点缺失或名称不唯一：{name}");
            let proxy = matching[0];
            ensure!(
                !matches!(
                    proxy.get("type").and_then(Value::as_str),
                    Some("direct" | "reject" | "dns" | "pass")
                ),
                "请选择实际代理节点：{name}"
            );
            ensure!(
                proxy.get("dialer-proxy").is_none(),
                "节点已有链式代理，请先断开旧链路：{name}"
            );
        }
        let groups = config
            .get("proxy-groups")
            .and_then(Value::as_sequence)
            .ok_or_else(|| anyhow::anyhow!("代理组不存在"))?;
        ensure!(
            !groups
                .iter()
                .any(|g| g.get("name").and_then(Value::as_str) == Some(ENTRY_GROUP)),
            "保留代理组名称冲突：{ENTRY_GROUP}"
        );
        // Disabling auto recovery removes its generated group. Keep the dialer
        // links valid so turning recovery off does not make configuration fail.
        if self.target_group != "GLOBAL" && self.target_group != GROUP {
            let group = groups
                .iter()
                .find(|g| g.get("name").and_then(Value::as_str) == Some(&self.target_group))
                .ok_or_else(|| anyhow::anyhow!("目标代理组不存在"))?;
            ensure!(
                group.get("type").and_then(Value::as_str) == Some("select"),
                "目标必须是手动选择代理组"
            );
            ensure!(
                group
                    .get("proxies")
                    .and_then(Value::as_sequence)
                    .is_some_and(|members| members.contains(&Value::from(self.exit_node.clone()))),
                "出口不在目标代理组中"
            );
        }
        Ok(exits)
    }
}

/// Called on a newly generated profile, after failover enhancement. Never changes
/// the downloaded subscription or erases unrelated dialer-proxy settings.
pub fn enhance(config: &mut Mapping, settings: &EntryPoolConfig, managed: bool) -> Result<()> {
    let exits = settings.validate(config)?;
    let mut group = Mapping::new();
    group.insert("name".into(), ENTRY_GROUP.into());
    group.insert("type".into(), "fallback".into());
    group.insert("proxies".into(), serde_yaml_ng::to_value(&settings.entries)?);
    // The entrance only needs ordinary connectivity. OpenAI access is checked
    // through the complete chain by auto-failover, not required of the entrance.
    group.insert("url".into(), "https://www.gstatic.com/generate_204".into());
    group.insert("expected-status".into(), "204".into());
    group.insert("interval".into(), 20.into());
    group.insert("timeout".into(), 5000.into());
    group.insert("lazy".into(), false.into());
    group.insert("max-failed-times".into(), 1.into());
    if let Some(proxies) = config.get_mut("proxies").and_then(Value::as_sequence_mut) {
        for proxy in proxies {
            if proxy
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| exits.iter().any(|exit| exit == name))
            {
                proxy
                    .as_mapping_mut()
                    .ok_or_else(|| anyhow::anyhow!("Invalid proxy"))?
                    .insert("dialer-proxy".into(), ENTRY_GROUP.into());
            }
        }
    }
    if let Some(groups) = config.get_mut("proxy-groups").and_then(Value::as_sequence_mut) {
        if managed {
            let managed_group = groups
                .iter_mut()
                .find(|g| g.get("name").and_then(Value::as_str) == Some(GROUP))
                .and_then(Value::as_mapping_mut)
                .ok_or_else(|| anyhow::anyhow!("自动恢复代理组不存在"))?;
            // Keep every automatic selection inside the exit pair, including after
            // a restart. Provider nodes and entrance nodes cannot become exits.
            managed_group.remove("use");
            managed_group.insert("proxies".into(), serde_yaml_ng::to_value(&exits)?);
        }
        groups.push(Value::Mapping(group));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    fn fixture() -> (Mapping, EntryPoolConfig) {
        let names = ["Entry A", "Entry B", "US 1.2.3.4 · 主线", "US 1.2.3.4 · 备用", "Other"];
        let mut config = Mapping::new();
        config.insert(
            "proxies".into(),
            Value::Sequence(
                names
                    .iter()
                    .map(|n| {
                        let mut p = Mapping::new();
                        p.insert("name".into(), (*n).into());
                        p.insert("type".into(), "ss".into());
                        Value::Mapping(p)
                    })
                    .collect(),
            ),
        );
        let mut g = Mapping::new();
        g.insert("name".into(), GROUP.into());
        g.insert("type".into(), "select".into());
        g.insert("proxies".into(), serde_yaml_ng::to_value(names).unwrap());
        config.insert("proxy-groups".into(), Value::Sequence(vec![Value::Mapping(g)]));
        (
            config,
            EntryPoolConfig {
                enabled: true,
                entries: vec![names[0].into(), names[1].into()],
                exit_node: names[2].into(),
                target_group: GROUP.into(),
                ..Default::default()
            },
        )
    }
    #[test]
    fn entrances_are_alternatives_and_both_exits_use_the_same_pool() {
        let (mut config, settings) = fixture();
        enhance(&mut config, &settings, true).unwrap();
        if let Ok(path) = std::env::var("ENTRY_POOL_TEST_FIXTURE") {
            std::fs::write(path, serde_json::to_vec(&config).unwrap()).unwrap();
        }
        assert!(config["proxies"][0].get("dialer-proxy").is_none());
        assert!(config["proxies"][1].get("dialer-proxy").is_none());
        for i in [2, 3] {
            assert_eq!(config["proxies"][i]["dialer-proxy"].as_str(), Some(ENTRY_GROUP));
        }
        assert!(config["proxies"][4].get("dialer-proxy").is_none());
        assert_eq!(config["proxy-groups"][0]["proxies"].as_sequence().unwrap().len(), 2);
        let group = &config["proxy-groups"][1];
        assert_eq!(group["type"].as_str(), Some("fallback"));
        assert_eq!(group["proxies"], serde_yaml_ng::to_value(&settings.entries).unwrap());
        assert_eq!(group["interval"].as_u64(), Some(20));
        assert_eq!(group["lazy"].as_bool(), Some(false));
    }
    #[test]
    fn disabling_recovery_keeps_exit_dialers_without_requiring_the_managed_group() {
        let (mut config, settings) = fixture();
        config.insert("proxy-groups".into(), Value::Sequence(vec![]));
        enhance(&mut config, &settings, false).unwrap();
        assert_eq!(config["proxies"][2]["dialer-proxy"].as_str(), Some(ENTRY_GROUP));
        assert_eq!(config["proxy-groups"].as_sequence().unwrap().len(), 1);
    }

    #[test]
    fn reject_missing_duplicate_and_cyclic_candidates_without_modifying_config() {
        for entries in [
            vec![],
            vec!["Missing"],
            vec!["Entry A", "Entry A"],
            vec!["US 1.2.3.4 · 备用"],
        ] {
            let (mut config, mut settings) = fixture();
            let before = config.clone();
            settings.entries = entries.into_iter().map(str::to_owned).collect();
            assert!(enhance(&mut config, &settings, true).is_err());
            assert_eq!(config, before);
        }
    }
    #[test]
    fn existing_user_chains_are_not_overwritten_and_regeneration_is_repeatable() {
        let (original, settings) = fixture();
        let mut first = original.clone();
        let mut second = original.clone();
        enhance(&mut first, &settings, true).unwrap();
        enhance(&mut second, &settings, true).unwrap();
        assert_eq!(first, second);
        let mut conflict = original.clone();
        conflict["proxies"][0]
            .as_mapping_mut()
            .unwrap()
            .insert("dialer-proxy".into(), settings.exit_node.clone().into());
        let before = conflict.clone();
        assert!(enhance(&mut conflict, &settings, true).is_err());
        assert_eq!(before, conflict);
    }
}
