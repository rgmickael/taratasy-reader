use crate::document::Document;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct AppState {
    pub docs: Vec<Document>,
    pub active: Option<PathBuf>,
    pub font_size: f32,
    pub show_summary: bool,
    pub dark: Option<bool>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            docs: Vec::new(),
            active: None,
            font_size: 19.0,
            show_summary: true,
            dark: None,
        }
    }
}

impl AppState {
    pub fn load() -> Self {
        std::fs::read_to_string(config_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = config_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn active_doc(&self) -> Option<&Document> {
        let a = self.active.as_ref()?;
        self.docs.iter().find(|d| &d.path == a)
    }

    pub fn doc_mut(&mut self, path: &Path) -> Option<&mut Document> {
        self.docs.iter_mut().find(|d| d.path == path)
    }

    pub fn close(&mut self, path: &Path) {
        let Some(idx) = self.docs.iter().position(|d| d.path == path) else { return };
        self.docs.remove(idx);
        if self.active.as_deref() == Some(path) {
            let next = idx.min(self.docs.len().saturating_sub(1));
            self.active = self.docs.get(next).map(|d| d.path.clone());
        }
    }
}

fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("taratasy-reader").join("state.json")
}
