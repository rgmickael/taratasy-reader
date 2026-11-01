use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Document {
    pub path: PathBuf,
    pub title: String,
    pub author: String,
    pub chapter: usize,
    pub scroll: f32,
    pub progress: f32,
    pub total_chapters: usize,
}
