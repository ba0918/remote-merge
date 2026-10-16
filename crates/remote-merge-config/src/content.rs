use remote_merge_core::{content::Interpretation, filter::matches_path_pattern};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RawContent {
    #[serde(default)]
    defaults: BTreeMap<String, String>,
    #[serde(default)]
    rules: Vec<RawRule>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    side: String,
    path: String,
    encoding: String,
}

#[derive(Debug, Clone, Default)]
pub struct ContentConfig {
    defaults: BTreeMap<String, Interpretation>,
    rules: Vec<Rule>,
}

#[derive(Debug, Clone)]
struct Rule {
    side: String,
    path: String,
    encoding: Interpretation,
}

impl ContentConfig {
    pub fn resolve(&self, side: &str, path: &str, cli: Option<Interpretation>) -> Interpretation {
        cli.or_else(|| {
            self.rules
                .iter()
                .rev()
                .find(|rule| rule.side == side && matches_path_pattern(path, &rule.path))
                .map(|rule| rule.encoding)
        })
        .or_else(|| self.defaults.get(side).copied())
        .unwrap_or_default()
    }
}

pub(crate) fn merge_content(
    global: Option<RawContent>,
    project: Option<RawContent>,
    servers: &BTreeMap<String, crate::ServerConfig>,
) -> anyhow::Result<ContentConfig> {
    let mut result = ContentConfig::default();
    for raw in [global, project].into_iter().flatten() {
        for (side, name) in raw.defaults {
            validate_side(&side, servers)?;
            result
                .defaults
                .insert(side, name.parse().map_err(anyhow::Error::msg)?);
        }
        for rule in raw.rules {
            validate_side(&rule.side, servers)?;
            result.rules.push(Rule {
                side: rule.side,
                path: rule.path,
                encoding: rule.encoding.parse().map_err(anyhow::Error::msg)?,
            });
        }
    }
    Ok(result)
}

fn validate_side(
    side: &str,
    servers: &BTreeMap<String, crate::ServerConfig>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        side == "local" || servers.contains_key(side),
        "unknown content side: {side}"
    );
    Ok(())
}
