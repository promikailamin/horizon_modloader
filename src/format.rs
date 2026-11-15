//! The `.hmmod` archive format: a deterministic ZIP with a JSON manifest and a
//! `payload/` tree whose every file is SHA-256 checked.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;
use zip::write::FileOptions;
use zip::{DateTime, ZipArchive, ZipWriter};

pub const FORMAT_VERSION: u32 = 1;
pub const MANIFEST_NAME: &str = "hmmod.json";
pub const PAYLOAD_DIR: &str = "payload";

#[derive(Debug)]
pub enum ManifestError {
    Invalid(String),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::Invalid(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for ManifestError {}

/// One mod that is compiled into this build.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ModEntry {
    pub id: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// The JSON manifest at the root of every `.hmmod`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HmmodManifest {
    pub format: u32,
    /// Only `prebuilt-game` is defined for format 1.
    pub kind: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub horizon_revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizon_version: Option<String>,
    pub target: String,
    pub profile: String,
    /// Changes whenever any input changes; mods with the same value can share
    /// one prebuilt game.
    pub build_id: String,
    /// Path inside `payload/`, with `/` separators.
    pub executable: String,
    /// SHA-256 of the `.hnmod` this package was converted from (if any).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_bundle: Option<String>,
    #[serde(default)]
    pub mods: Vec<ModEntry>,
    /// Relative path under `payload/` -> lowercase hex SHA-256.
    pub files: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn valid_rel_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || path.contains(':') {
        return false;
    }
    let p = Path::new(path);
    if p.is_absolute() {
        return false;
    }
    p.components().all(|c| matches!(c, Component::Normal(_)))
}

impl HmmodManifest {
    pub fn validate(&self) -> Result<(), ManifestError> {
        let err = |s: &str| Err(ManifestError::Invalid(s.to_string()));
        if self.format != FORMAT_VERSION {
            return err("unsupported hmmod format (expected 1)");
        }
        if self.kind != "prebuilt-game" {
            return err("unsupported hmmod kind (format 1 defines only \"prebuilt-game\")");
        }
        if !valid_id(&self.id) {
            return err("id must match [a-z0-9_]{1,64}");
        }
        if self.name.trim().is_empty() {
            return err("name must not be empty");
        }
        if self.version.trim().is_empty() {
            return err("version must not be empty");
        }
        if self.horizon_revision.len() < 7
            || !self.horizon_revision.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return err("horizon_revision must be a hexadecimal commit id");
        }
        if self.target.trim().is_empty() {
            return err("target must not be empty");
        }
        if self.profile.trim().is_empty() {
            return err("profile must not be empty");
        }
        if self.build_id.trim().is_empty() {
            return err("build_id must not be empty");
        }
        if !valid_rel_path(&self.executable) {
            return err("executable must be a relative path inside payload/");
        }
        if !self.files.contains_key(&self.executable) {
            return err("executable is not listed in files");
        }
        for key in self.files.keys() {
            if !valid_rel_path(key) {
                return Err(ManifestError::Invalid(format!(
                    "unsafe path in files: {key}"
                )));
            }
        }
        for m in &self.mods {
            if !valid_id(&m.id) {
                return Err(ManifestError::Invalid(format!("invalid mod id: {}", m.id)));
            }
            if m.version.trim().is_empty() {
                return Err(ManifestError::Invalid(format!(
                    "mod {} has no version",
                    m.id
                )));
            }
        }
        Ok(())
    }

    pub fn executable_path(&self) -> PathBuf {
        Path::new(PAYLOAD_DIR).join(&self.executable)
    }

    pub fn archive_id(&self) -> String {
        let short = &self.build_id[..self.build_id.len().min(12)];
        format!("{}-{}-{short}", self.id, self.version)
    }

    pub fn read_from_archive<R: Read + io::Seek>(
        archive: &mut ZipArchive<R>,
    ) -> anyhow::Result<Self> {
        let mut entry = archive
            .by_name(MANIFEST_NAME)
            .map_err(|_| anyhow::anyhow!("{MANIFEST_NAME} is missing from the archive"))?;
        if entry.size() > 4 * 1024 * 1024 {
            anyhow::bail!("{MANIFEST_NAME} is unreasonably large");
        }
        let mut text = String::new();
        entry.read_to_string(&mut text)?;
        let manifest: Self = serde_json::from_str(&text)
            .map_err(|e| anyhow::anyhow!("{MANIFEST_NAME} is not valid JSON: {e}"))?;
        manifest.validate()?;
        Ok(manifest)
    }
}

pub fn sha256_file(path: &Path) -> anyhow::Result<String> {
    let mut f = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn current_target() -> &'static str {
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    {
        "x86_64-unknown-linux-gnu"
    }
    #[cfg(all(target_arch = "aarch64", target_os = "linux"))]
    {
        "aarch64-unknown-linux-gnu"
    }
    #[cfg(all(target_arch = "x86_64", target_os = "macos"))]
    {
        "x86_64-apple-darwin"
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    {
        "aarch64-apple-darwin"
    }
    #[cfg(all(target_arch = "x86_64", target_os = "windows"))]
    {
        "x86_64-pc-windows-msvc"
    }
    #[cfg(all(target_arch = "aarch64", target_os = "windows"))]
    {
        "aarch64-pc-windows-msvc"
    }
}

pub fn executable_name() -> &'static str {
    if cfg!(windows) {
        "horizon-client.exe"
    } else {
        "horizon-client"
    }
}

/// Inputs to a new package. `payload_files` are (source path, path inside
/// `payload/`). Sorted before writing, so identical inputs give identical ZIPs.
pub struct PackOptions {
    pub manifest_base: HmmodManifest,
    pub payload_files: Vec<(PathBuf, String)>,
}

pub fn pack_hmmod(out: &Path, opts: PackOptions) -> anyhow::Result<HmmodManifest> {
    let mut manifest = opts.manifest_base;
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut files = BTreeMap::new();
    let mut entries = opts.payload_files;
    entries.sort_by(|a, b| a.1.cmp(&b.1));
    for (_, rel) in &entries {
        if !valid_rel_path(rel) {
            anyhow::bail!("unsafe payload path: {rel}");
        }
        if files.contains_key(rel) {
            anyhow::bail!("duplicate payload path: {rel}");
        }
        files.insert(rel.clone(), String::new());
    }
    if !files.contains_key(&manifest.executable) {
        anyhow::bail!("payload does not contain the declared executable");
    }

    for (src, rel) in &entries {
        let hash = sha256_file(src)?;
        files.insert(rel.clone(), hash);
    }
    manifest.files = files;
    manifest.validate()?;

    let tmp = out.with_extension(format!("hmmod.tmp-{}", std::process::id()));
    let file = File::create(&tmp)?;
    let mut zip = ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644);

    zip.start_file(MANIFEST_NAME, options)?;
    let json = serde_json::to_vec_pretty(&manifest)?;
    zip.write_all(&json)?;
    zip.write_all(b"\n")?;

    for (src, rel) in &entries {
        let archive_path = format!("{PAYLOAD_DIR}/{rel}");
        let mode = if rel == &manifest.executable {
            0o755
        } else {
            0o644
        };
        zip.start_file(archive_path, options.unix_permissions(mode))?;
        let mut f = File::open(src)?;
        io::copy(&mut f, &mut zip)?;
    }
    zip.finish()?;
    fs::rename(&tmp, out)?;
    Ok(manifest)
}

/// Extract and fully verify an `.hmmod` into `dest` (which must not exist).
pub fn extract_hmmod(archive_path: &Path, dest: &Path) -> anyhow::Result<HmmodManifest> {
    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file)?;
    let manifest = HmmodManifest::read_from_archive(&mut archive)?;

    if dest.exists() {
        anyhow::bail!("destination already exists: {}", dest.display());
    }
    let parent = dest.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".{}.tmp-{}",
        dest.file_name().and_then(|s| s.to_str()).unwrap_or("hmmod"),
        std::process::id()
    ));
    if tmp.exists() {
        fs::remove_dir_all(&tmp)?;
    }
    fs::create_dir_all(&tmp)?;

    let result = (|| -> anyhow::Result<()> {
        let mut seen = BTreeMap::new();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().to_string();
            if name == MANIFEST_NAME || name.ends_with('/') {
                continue;
            }
            let enclosed = entry
                .enclosed_name()
                .ok_or_else(|| anyhow::anyhow!("archive contains an unsafe path: {name}"))?;
            if !name.starts_with(&format!("{PAYLOAD_DIR}/")) {
                anyhow::bail!("unexpected file outside payload/: {name}");
            }
            let rel = enclosed
                .strip_prefix(PAYLOAD_DIR)
                .map_err(|_| anyhow::anyhow!("bad payload path: {name}"))?;
            let rel_str = rel
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("non-UTF-8 payload path: {name}"))?
                .replace('\\', "/");
            if !valid_rel_path(&rel_str) {
                anyhow::bail!("unsafe payload path: {rel_str}");
            }
            if seen.insert(rel_str.clone(), ()).is_some() {
                anyhow::bail!("duplicate payload file: {rel_str}");
            }
            let expected = manifest.files.get(&rel_str).ok_or_else(|| {
                anyhow::anyhow!("payload file is not hashed in manifest: {rel_str}")
            })?;

            let out_path = tmp.join(rel);
            if let Some(p) = out_path.parent() {
                fs::create_dir_all(p)?;
            }
            let mut out = File::create(&out_path)?;
            let mut hasher = Sha256::new();
            let mut buf = [0u8; 64 * 1024];
            loop {
                let n = entry.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buf[..n]);
                out.write_all(&buf[..n])?;
            }
            let actual = hex::encode(hasher.finalize());
            if &actual != expected {
                anyhow::bail!(
                    "checksum mismatch for {rel_str}: manifest {expected}, archive {actual}"
                );
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = if rel_str == manifest.executable {
                    0o755
                } else {
                    0o644
                };
                fs::set_permissions(&out_path, fs::Permissions::from_mode(mode))?;
            }
        }
        for declared in manifest.files.keys() {
            if !seen.contains_key(declared) {
                anyhow::bail!("manifest lists a file missing from the archive: {declared}");
            }
        }
        Ok(())
    })();

    if let Err(e) = result {
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, dest)?;
    Ok(manifest)
}

/// Hash every regular file under `root`, returning sorted relative paths.
pub fn tree_files(root: &Path) -> anyhow::Result<Vec<(PathBuf, String)>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root).follow_links(false).sort_by_file_name() {
        let entry = entry?;
        if entry.file_type().is_symlink() {
            anyhow::bail!("symbolic links are not allowed: {}", entry.path().display());
        }
        if entry.file_type().is_file() {
            let rel = entry
                .path()
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            files.push((entry.path().to_path_buf(), rel));
        }
    }
    Ok(files)
}
