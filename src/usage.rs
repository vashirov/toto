use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub struct Usage {
    counts: HashMap<String, u64>,
    path: Option<PathBuf>,
}

pub fn make_key(tags: &str, comment: &str) -> String {
    format!("{}//{}", tags.trim(), comment.trim())
}

impl Usage {
    pub fn load() -> Self {
        let path = usage_path();
        let counts = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| parse_usage_toml(&s))
            .unwrap_or_default();
        Self { counts, path }
    }

    pub fn get(&self, key: &str) -> u64 {
        self.counts.get(key).copied().unwrap_or(0)
    }

    pub fn record(&mut self, key: &str) {
        let count = self.counts.entry(key.to_string()).or_insert(0);
        *count = count.saturating_add(1);
        let Some(path) = &self.path else { return };
        let Some(parent) = path.parent() else { return };
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
        let mut text = String::from("[usage]\n");
        let mut keys: Vec<&String> = self.counts.keys().collect();
        keys.sort();
        for key in keys {
            let escaped = key.replace('\\', "\\\\").replace('"', "\\\"");
            text.push_str(&format!("\"{escaped}\" = {}\n", self.counts[key]));
        }
        let tmp = parent.join(".usage.toml.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(tmp, path);
        }
    }
}

fn parse_usage_toml(s: &str) -> Option<HashMap<String, u64>> {
    let table: toml::Value = s.parse().ok()?;
    let usage = table.get("usage")?.as_table()?;
    Some(
        usage
            .iter()
            .filter_map(|(key, value)| Some((key.clone(), value.as_integer()? as u64)))
            .collect(),
    )
}

fn usage_path() -> Option<PathBuf> {
    crate::files::default_data_dir().map(|mut p| {
        p.push("usage.toml");
        p
    })
}
