//! Reading Horizon's source-only `.hnmod` bundles.

use crate::format::{sha256_bytes, sha256_file, HmmodManifest, ModEntry};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use zip::ZipArchive;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NativeManifest {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub game: Option<String>,
    #[serde(default)]
    pub horizon: String,
    #[serde(default)]
    pub mixins: Option<String>,
    #[serde(default)]
    pub schema: Option<u32>,
    #[serde(default)]
    pub repository: Option<String>,
}

#[derive(Clone, Debug)]
pub struct HnmodBundle {
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: Vec<u8>,
    pub manifest: NativeManifest,
}

fn safe_relative(name: &str) -> Option<PathBuf> {
    if name.is_empty() || name.starts_with('/') || name.contains('\\') {
        return None;
    }
    let p = Path::new(name);
    if p.is_absolute() || !p.components().all(|c| matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(p.to_path_buf())
}

impl HnmodBundle {
    pub fn open(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
        if bytes.len() > 128 * 1024 * 1024 {
            anyhow::bail!("{} is larger than 128 MiB", path.display());
        }
        let sha256 = sha256_file(path)?;
        let cursor = std::io::Cursor::new(&bytes);
        let mut zip = ZipArchive::new(cursor)
            .with_context(|| format!("{} is not a .hnmod zip archive", path.display()))?;
        let mut manifest_text = String::new();
        {
            let mut entry = zip
                .by_name("horizon-native.json")
                .map_err(|_| anyhow::anyhow!("{} has no horizon-native.json", path.display()))?;
            entry.read_to_string(&mut manifest_text)?;
        }
        let manifest: NativeManifest = serde_json::from_str(&manifest_text)
            .with_context(|| format!("{} has an invalid horizon-native.json", path.display()))?;
        if manifest.id.is_empty() {
            anyhow::bail!("{}: horizon-native.json has no mod id", path.display());
        }
        if manifest.version.is_empty() {
            anyhow::bail!("{}: horizon-native.json has no version", path.display());
        }
        if manifest.horizon.len() < 7 {
            anyhow::bail!(
                "{}: horizon-native.json has no Horizon revision pin",
                path.display()
            );
        }
        Ok(Self {
            path: path.to_path_buf(),
            sha256,
            bytes,
            manifest,
        })
    }

    pub fn mod_entry(&self) -> ModEntry {
        ModEntry {
            id: self.manifest.id.clone(),
            version: self.manifest.version.clone(),
            name: self.manifest.name.clone(),
        }
    }

    /// Verify that every file lives under `mixins/<id>/` and write that folder
    /// to `dest`. This is the bundle layout horizon-mixins expects.
    pub fn extract_bundle(&self, dest: &Path) -> Result<()> {
        let cursor = std::io::Cursor::new(&self.bytes);
        let mut zip = ZipArchive::new(cursor)?;
        let prefix = format!("mixins/{}/", self.manifest.id);
        let mut found = false;
        if dest.exists() {
            fs::remove_dir_all(dest)?;
        }
        fs::create_dir_all(dest)?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let name = entry.name().to_string();
            if entry.is_dir() {
                continue;
            }
            let Some(rel) = name.strip_prefix(&prefix) else {
                // `.hnmod` files outside `mixins/<id>/` are metadata such as
                // `horizon-native.json`; they are not part of the mixin bundle.
                continue;
            };
            let rel = safe_relative(rel)
                .ok_or_else(|| anyhow::anyhow!("unsafe path in {}: {name}", self.path.display()))?;
            let out = dest.join(rel);
            if let Some(p) = out.parent() {
                fs::create_dir_all(p)?;
            }
            let mut f = fs::File::create(&out)?;
            std::io::copy(&mut entry, &mut f)?;
            found = true;
        }
        if !found {
            anyhow::bail!(
                "{} has no files under mixins/{}/",
                self.path.display(),
                self.manifest.id
            );
        }
        Ok(())
    }

    /// Base manifest fields for a package made from this bundle.
    pub fn manifest_base(&self, profile: &str, target: &str, build_id: &str) -> HmmodManifest {
        HmmodManifest {
            format: crate::format::FORMAT_VERSION,
            kind: "prebuilt-game".to_string(),
            id: self.manifest.id.clone(),
            name: self
                .manifest
                .name
                .clone()
                .unwrap_or_else(|| self.manifest.id.clone()),
            version: self.manifest.version.clone(),
            horizon_revision: self.manifest.horizon.clone(),
            horizon_version: self.manifest.game.clone().map(|g| format!("Minecraft {g}")),
            target: target.to_string(),
            profile: profile.to_string(),
            build_id: build_id.to_string(),
            executable: crate::store::default_executable(),
            source_sha256: Some(self.sha256.clone()),
            source_bundle: Some(
                self.path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("source.hnmod")
                    .to_string(),
            ),
            mods: vec![self.mod_entry()],
            files: Default::default(),
            notes: Some("Precompiled from the Horizon source pinned by the .hnmod.".to_string()),
        }
    }
}

pub fn combined_source_sha(bundles: &[HnmodBundle]) -> String {
    let mut parts = Vec::new();
    for b in bundles {
        parts.push(b.sha256.clone());
    }
    sha256_bytes(parts.join(":").as_bytes())
}
