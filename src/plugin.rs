// Copyright (c) 2026- Masaki Ishii
// Copyright (c) 2026- Small Gear Lab
// SPDX-License-Identifier: MIT OR Apache-2.0

use anyhow::Result;
use gettext::Catalog;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};

use crate::i18n::{preferred_locale_candidates, tr};

pub type PluginId = &'static str;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub group: Option<PluginManifestGroup>,
    #[serde(default)]
    pub i18n_domain: Option<String>,
    #[serde(default)]
    pub custom_fields: Vec<PluginCustomField>,
    #[serde(skip)]
    pub root_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifestGroup {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginCustomField {
    pub path: String,
    pub label: String,
    #[serde(default)]
    pub placement: PluginFieldPlacement,
    #[serde(default)]
    pub renderer: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginFieldPlacement {
    Left,
    #[default]
    Right,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderBlockKind {
    Text,
    Info,
    Code,
    Quote,
    Rule,
}

impl RenderBlockKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Info => "info",
            Self::Code => "code",
            Self::Quote => "quote",
            Self::Rule => "rule",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderBlock {
    pub kind: RenderBlockKind,
    pub title: Option<String>,
    pub text: String,
    pub children: Vec<RenderBlock>,
}

impl RenderBlock {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            kind: RenderBlockKind::Text,
            title: None,
            text: text.into(),
            children: Vec::new(),
        }
    }

    pub fn info(
        title: Option<String>,
        text: impl Into<String>,
        children: Vec<RenderBlock>,
    ) -> Self {
        Self {
            kind: RenderBlockKind::Info,
            title,
            text: text.into(),
            children,
        }
    }

    pub fn code(text: impl Into<String>) -> Self {
        Self {
            kind: RenderBlockKind::Code,
            title: None,
            text: text.into(),
            children: Vec::new(),
        }
    }

    pub fn quote(text: impl Into<String>, children: Vec<RenderBlock>) -> Self {
        Self {
            kind: RenderBlockKind::Quote,
            title: None,
            text: text.into(),
            children,
        }
    }

    pub fn rule() -> Self {
        Self {
            kind: RenderBlockKind::Rule,
            title: None,
            text: String::new(),
            children: Vec::new(),
        }
    }

    pub fn into_value(self) -> Value {
        json!({
            "kind": self.kind.as_str(),
            "title": self.title,
            "text": self.text,
            "children": self.children.into_iter().map(Self::into_value).collect::<Vec<_>>(),
        })
    }
}

#[derive(Debug, Clone, Default)]
pub struct PluginExtra {
    namespaces: Map<String, Value>,
}

impl PluginExtra {
    pub fn insert(&mut self, plugin_id: PluginId, key: impl Into<String>, value: Value) {
        self.namespace_mut(plugin_id).insert(key.into(), value);
    }

    pub fn get(&self, plugin_id: &str, key: &str) -> Option<&Value> {
        self.get_namespace(plugin_id)
            .and_then(|namespace| namespace.get(key))
    }

    pub fn get_namespace(&self, plugin_id: &str) -> Option<&Map<String, Value>> {
        self.namespaces.get(plugin_id).and_then(Value::as_object)
    }

    pub fn into_map(self) -> Map<String, Value> {
        self.namespaces
    }

    fn namespace_mut(&mut self, plugin_id: PluginId) -> &mut Map<String, Value> {
        let entry = self
            .namespaces
            .entry(plugin_id.to_string())
            .or_insert_with(|| Value::Object(Map::new()));

        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }

        entry
            .as_object_mut()
            .expect("plugin namespace should always be an object")
    }
}

pub fn plugin_manifests() -> Result<Vec<PluginManifest>> {
    let manifests = plugin_manifests_in_dir(&plugin_root_dir())?;
    #[cfg(test)]
    {
        let mut merged = manifests;
        for manifest in plugin_manifests_in_dir(&test_plugin_root_dir())? {
            if merged.iter().any(|existing| existing.id == manifest.id) {
                continue;
            }
            merged.push(manifest);
        }
        merged.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(merged)
    }

    #[cfg(not(test))]
    Ok(manifests)
}

fn plugin_manifests_in_dir(root: &Path) -> Result<Vec<PluginManifest>> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut manifests = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let plugin_dir = entry.path();
        if !plugin_dir.is_dir() {
            continue;
        }

        let manifest_path = plugin_dir.join("manifest.toml");
        if !manifest_path.is_file() {
            continue;
        }

        let content = fs::read_to_string(&manifest_path)?;
        let mut manifest: PluginManifest = toml::from_str(&content)?;
        manifest.root_dir = plugin_dir;
        manifests.push(manifest);
    }

    manifests.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(manifests)
}

pub fn tr_plugin(manifest: &PluginManifest, message: &str) -> String {
    plugin_catalog(manifest)
        .map(|catalog| catalog.gettext(message).to_string())
        .unwrap_or_else(|| tr(message))
}

fn plugin_catalog(manifest: &PluginManifest) -> Option<Catalog> {
    let locale_root = manifest.root_dir.join("locale");
    let domain = manifest.i18n_domain.as_deref().unwrap_or(&manifest.id);

    for locale in preferred_locale_candidates() {
        let path = locale_root
            .join(&locale)
            .join("LC_MESSAGES")
            .join(format!("{domain}.mo"));
        if path.is_file() {
            return parse_catalog(&path).ok();
        }
    }

    None
}

fn parse_catalog(path: &Path) -> Result<Catalog> {
    let file = File::open(path)?;
    Ok(Catalog::parse(file)?)
}

fn plugin_root_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("plugins")
}

#[cfg(test)]
fn test_plugin_root_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("plugins")
}

#[cfg(test)]
mod tests {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs as unix_fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::Value;

    use super::{PluginExtra, PluginFieldPlacement, RenderBlock, plugin_manifests_in_dir};

    #[test]
    fn stores_values_under_plugin_namespaces() {
        let mut extra = PluginExtra::default();
        extra.insert("chatwork", "requester", Value::String("佐藤".into()));
        extra.insert(
            "chatwork",
            "template_kind",
            Value::String("company_request".into()),
        );

        assert_eq!(
            extra.get("chatwork", "requester"),
            Some(&Value::String("佐藤".into()))
        );
        assert_eq!(
            extra.get("chatwork", "template_kind"),
            Some(&Value::String("company_request".into()))
        );
        assert!(extra.get_namespace("missing").is_none());
    }

    #[test]
    fn render_block_serializes_into_expected_shape() {
        assert_eq!(
            RenderBlock::info(
                Some("改修依頼".into()),
                "詳細本文",
                vec![RenderBlock::code("echo hello")],
            )
            .into_value(),
            serde_json::json!({
                "kind": "info",
                "title": "改修依頼",
                "text": "詳細本文",
                "children": [{
                    "kind": "code",
                    "title": null,
                    "text": "echo hello",
                    "children": [],
                }],
            })
        );
        assert_eq!(
            RenderBlock::code("echo hello").into_value(),
            serde_json::json!({
                "kind": "code",
                "title": null,
                "text": "echo hello",
                "children": [],
            })
        );
        assert_eq!(
            RenderBlock::quote("quoted", vec![RenderBlock::code("echo nested")]).into_value(),
            serde_json::json!({
                "kind": "quote",
                "title": null,
                "text": "quoted",
                "children": [{
                    "kind": "code",
                    "title": null,
                    "text": "echo nested",
                    "children": [],
                }],
            })
        );
        assert_eq!(
            RenderBlock::rule().into_value(),
            serde_json::json!({
                "kind": "rule",
                "title": null,
                "text": "",
                "children": [],
            })
        );
    }

    #[test]
    fn loads_plugin_manifests() {
        let root = unique_temp_dir("taskforce-plugin-manifests");
        let plugin_dir = root.join("chatwork");
        fs::create_dir_all(&plugin_dir).expect("plugin dir");
        fs::write(
            plugin_dir.join("manifest.toml"),
            r#"
id = "chatwork"
name = "Chatwork"

[[custom_fields]]
path = "render_blocks"
label = "詳細"
placement = "left"

[[custom_fields]]
path = "source"
label = "Source"
placement = "hidden"
"#,
        )
        .expect("manifest");

        let manifests = plugin_manifests_in_dir(&root).expect("plugin manifests");
        let chatwork = manifests.first().expect("chatwork manifest");

        assert_eq!(chatwork.name, "Chatwork");
        assert!(chatwork.custom_fields.iter().any(|field| {
            field.path == "render_blocks" && field.placement == PluginFieldPlacement::Left
        }));
        assert!(chatwork.custom_fields.iter().any(|field| {
            field.path == "source" && field.placement == PluginFieldPlacement::Hidden
        }));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn loads_plugin_manifests_from_symlinked_directories() {
        let root = unique_temp_dir("taskforce-plugin-manifests-link");
        let catalog_root = unique_temp_dir("taskforce-plugin-manifests-catalog");
        fs::create_dir_all(&root).expect("root dir");
        let plugin_dir = catalog_root.join("github-repo");
        fs::create_dir_all(&plugin_dir).expect("plugin dir");
        fs::write(
            plugin_dir.join("manifest.toml"),
            r#"
id = "github-repo"
name = "GitHub Repo"

[group]
id = "github"
label = "GitHub"

[[custom_fields]]
path = "repository"
label = "Repository"
placement = "right"
"#,
        )
        .expect("manifest");

        unix_fs::symlink(&plugin_dir, root.join("github-repo")).expect("symlink");

        let manifests = plugin_manifests_in_dir(&root).expect("plugin manifests");
        let manifest = manifests.first().expect("github-repo manifest");

        assert_eq!(manifest.id, "github-repo");
        assert_eq!(
            manifest.group.as_ref().map(|group| group.id.as_str()),
            Some("github")
        );

        fs::remove_dir_all(root).expect("cleanup root");
        fs::remove_dir_all(catalog_root).expect("cleanup catalog");
    }

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{nanos}"))
    }
}
