//! Local mod store, installation and launching.

use crate::format::{
    current_target, executable_name, extract_hmmod, sha256_bytes, sha256_file, HmmodManifest,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::ZipArchive;

#[derive(Clone, Debug)]
pub struct LoaderHome {
    pub root: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InstalledMod {
    pub archive: PathBuf,
    pub archive_sha256: String,
    pub installed_dir: PathBuf,
    pub manifest: HmmodManifest,
}

impl LoaderHome {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn default_home() -> Self {
        if let Ok(p) = std::env::var("HORIZON_HMMOD_HOME") {
            return Self::new(p);
        }
        if let Ok(p) = std::env::var("XDG_DATA_HOME") {
            return Self::new(PathBuf::from(p).join("horizon-hmmod"));
        }
        #[cfg(windows)]
        {
            if let Ok(p) = std::env::var("APPDATA") {
                return Self::new(PathBuf::from(p).join("horizon-hmmod"));
            }
        }
        if let Ok(p) = std::env::var("HOME") {
            return Self::new(PathBuf::from(p).join(".local/share/horizon-hmmod"));
        }
        Self::new(".horizon-hmmod")
    }

    pub fn mods_dir(&self) -> PathBuf {
        self.root.join("mods")
    }
    pub fn store_dir(&self) -> PathBuf {
        self.root.join("store")
    }
    pub fn instances_dir(&self) -> PathBuf {
        self.root.join("instances")
    }
    pub fn work_dir(&self) -> PathBuf {
        self.root.join("work")
    }
    pub fn tools_dir(&self) -> PathBuf {
        self.root.join("tools")
    }

    pub fn ensure(&self) -> Result<()> {
        for d in [
            self.mods_dir(),
            self.store_dir(),
            self.instances_dir(),
            self.work_dir(),
            self.tools_dir(),
        ] {
            fs::create_dir_all(&d)?;
        }
        Ok(())
    }

    /// Read a manifest from an `.hmmod` without extracting it.
    pub fn inspect(&self, archive: &Path) -> Result<HmmodManifest> {
        let file = fs::File::open(archive)
            .with_context(|| format!("cannot open {}", archive.display()))?;
        let mut zip = ZipArchive::new(file)
            .with_context(|| format!("{} is not a zip archive", archive.display()))?;
        HmmodManifest::read_from_archive(&mut zip)
    }

    /// Install an `.hmmod`: verify it, extract it to the store, and place a copy
    /// in the `mods/` folder. Re-installing the same build is a no-op.
    pub fn install(&self, archive: &Path) -> Result<InstalledMod> {
        self.ensure()?;
        let archive_sha = sha256_file(archive)?;
        let manifest = self.inspect(archive)?;
        let dir_name = format!(
            "{}-{}",
            manifest.archive_id(),
            &archive_sha[..12.min(archive_sha.len())]
        );
        let installed_dir = self.store_dir().join(&dir_name);

        if !installed_dir.exists() {
            let tmp_dir = self.store_dir().join(format!(".{dir_name}.part"));
            if tmp_dir.exists() {
                fs::remove_dir_all(&tmp_dir)?;
            }
            extract_hmmod(archive, &tmp_dir)?;
            fs::rename(&tmp_dir, &installed_dir)?;
        }

        let dest_archive = self
            .mods_dir()
            .join(format!("{}-{}.hmmod", manifest.id, manifest.version));
        if !dest_archive.exists() || sha256_file(&dest_archive)? != archive_sha {
            let tmp = dest_archive.with_extension("hmmod.tmp");
            fs::copy(archive, &tmp)?;
            fs::rename(&tmp, &dest_archive)?;
        }

        Ok(InstalledMod {
            archive: dest_archive,
            archive_sha256: archive_sha,
            installed_dir,
            manifest,
        })
    }

    pub fn list(&self) -> Result<Vec<InstalledMod>> {
        self.ensure()?;
        let mut out = Vec::new();
        for entry in fs::read_dir(self.mods_dir())? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("hmmod") {
                continue;
            }
            let manifest = match self.inspect(&path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let sha = sha256_file(&path)?;
            let dir_name = format!("{}-{}", manifest.archive_id(), &sha[..12.min(sha.len())]);
            out.push(InstalledMod {
                archive: path,
                archive_sha256: sha,
                installed_dir: self.store_dir().join(dir_name),
                manifest,
            });
        }
        out.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
        Ok(out)
    }

    pub fn get(&self, id: &str) -> Result<InstalledMod> {
        let mods = self.list()?;
        let mut matches = mods.into_iter().filter(|m| m.manifest.id == id);
        let first = matches
            .next()
            .ok_or_else(|| anyhow::anyhow!("mod {id:?} is not installed"))?;
        if matches.next().is_some() {
            anyhow::bail!(
                "more than one installed archive for {id:?}; remove the extra copy from {}",
                self.mods_dir().display()
            );
        }
        Ok(first)
    }

    pub fn remove(&self, id: &str) -> Result<Vec<PathBuf>> {
        let matches: Vec<_> = self
            .list()?
            .into_iter()
            .filter(|m| m.manifest.id == id)
            .collect();
        if matches.is_empty() {
            anyhow::bail!("mod {id:?} is not installed");
        }
        let mut removed = Vec::new();
        for m in matches {
            if m.archive.exists() {
                fs::remove_file(&m.archive)?;
                removed.push(m.archive);
            }
            if m.installed_dir.exists() {
                fs::remove_dir_all(&m.installed_dir)?;
                removed.push(m.installed_dir);
            }
        }
        Ok(removed)
    }

    pub fn verify(&self, id: &str) -> Result<InstalledMod> {
        let installed = self.get(id)?;
        let m = &installed.manifest;
        if m.target != current_target() {
            anyhow::bail!(
                "mod {} targets {}, this computer is {}",
                m.id,
                m.target,
                current_target()
            );
        }
        if !installed.installed_dir.exists() {
            anyhow::bail!(
                "mod {} is not unpacked; run `hmloader install {}`",
                m.id,
                installed.archive.display()
            );
        }
        for (rel, expected) in &m.files {
            let path = installed.installed_dir.join(rel);
            if !path.is_file() {
                anyhow::bail!("missing file in installed mod: {}", path.display());
            }
            let actual = sha256_file(&path)?;
            if &actual != expected {
                anyhow::bail!("checksum mismatch for {}", path.display());
            }
        }
        Ok(installed)
    }

    pub fn instance_dir(&self, id: &str) -> PathBuf {
        self.instances_dir().join(id)
    }

    /// Select the mod to launch. Multiple mods are allowed only when they were
    /// built together (same `build_id`), because each `.hmmod` carries a whole
    /// prebuilt client.
    pub fn select(&self, id: Option<&str>) -> Result<InstalledMod> {
        let mods = self.list()?;
        if let Some(id) = id {
            return self.get(id);
        }
        match mods.len() {
            0 => anyhow::bail!(
                "no .hmmod is installed in {}; run `hmloader install <file.hmmod>`",
                self.mods_dir().display()
            ),
            1 => Ok(mods.into_iter().next().unwrap()),
            _ => {
                let build = mods[0].manifest.build_id.clone();
                if mods.iter().all(|m| m.manifest.build_id == build) {
                    Ok(mods.into_iter().next().unwrap())
                } else {
                    let names: Vec<_> = mods.iter().map(|m| m.manifest.id.as_str()).collect();
                    anyhow::bail!(
                        "several mods were built separately ({}); run `hmloader launch <id>` to choose one, \
                         or build them together into one .hmmod",
                        names.join(", ")
                    )
                }
            }
        }
    }

    pub fn launch(
        &self,
        id: Option<&str>,
        resources: Option<&Path>,
        game_args: &[String],
        dry_run: bool,
    ) -> Result<()> {
        let installed = self.select(id)?;
        let installed = self.verify(&installed.manifest.id)?;
        let manifest = &installed.manifest;
        if manifest.target != current_target() {
            anyhow::bail!(
                "this .hmmod was built for {}; this computer is {}",
                manifest.target,
                current_target()
            );
        }
        let exe = installed.installed_dir.join(&manifest.executable);
        if !exe.is_file() {
            anyhow::bail!("prebuilt client is missing: {}", exe.display());
        }
        let instance = self.instance_dir(&manifest.id);
        fs::create_dir_all(&instance)?;

        let mut cmd = Command::new(&exe);
        cmd.current_dir(&instance);
        cmd.env("HORIZON_HMMOD_ID", &manifest.id);
        cmd.env("HORIZON_HMMOD_BUILD", &manifest.build_id);
        if let Some(resources) = resources {
            let resources = fs::canonicalize(resources)
                .with_context(|| format!("resources folder not found: {}", resources.display()))?;
            cmd.arg("--resources").arg(resources);
        } else if let Some(resources) = discover_resources(&instance) {
            println!("  resources: {}", resources.display());
            cmd.arg("--resources").arg(resources);
        }
        for arg in game_args {
            cmd.arg(arg);
        }

        println!(
            "Launching {} {} ({} {})",
            manifest.name,
            manifest.version,
            manifest.id,
            &manifest.build_id[..12.min(manifest.build_id.len())]
        );
        println!("  client:   {}", exe.display());
        println!("  instance: {}", instance.display());
        if dry_run {
            println!("  command:  {:?}", cmd);
            return Ok(());
        }
        let status = cmd
            .status()
            .with_context(|| format!("cannot start {}", exe.display()))?;
        if !status.success() {
            anyhow::bail!("Horizon exited with {}", status);
        }
        Ok(())
    }

    /// Check that an installed game has prepared resources beside it. Kept as a
    /// helper for `doctor`.
    pub fn find_resources_in(version_dir: &Path) -> Option<PathBuf> {
        let candidates = [
            version_dir.join("resources"),
            version_dir.join("horizon-resources"),
            version_dir.join("game").join("resources"),
        ];
        candidates.into_iter().find(|p| p.exists())
    }

    /// Import a built client into the official Horizon launcher as its own
    /// version. The launcher then supplies its prepared resources and account.
    pub fn import_into_launcher(
        &self,
        id: &str,
        launcher: &Path,
        base_version: Option<&str>,
    ) -> Result<()> {
        let installed = self.verify(id)?;
        let manifest = &installed.manifest;
        let exe = installed.installed_dir.join(&manifest.executable);
        let mut cmd = Command::new(launcher);
        cmd.arg("import-build").arg(&exe).arg("--trust-unsigned");
        if let Some(v) = base_version {
            cmd.arg("--version").arg(v);
        }
        let status = cmd
            .status()
            .with_context(|| format!("cannot run {}", launcher.display()))?;
        if !status.success() {
            anyhow::bail!("horizon-launcher import-build failed with {status}");
        }
        Ok(())
    }
}

/// Read a file's bytes with a size cap, used by tests and diagnostics.
pub fn read_capped(path: &Path, cap: u64) -> Result<Vec<u8>> {
    let mut f = fs::File::open(path)?;
    let mut buf = Vec::new();
    f.by_ref().take(cap + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > cap {
        anyhow::bail!("file is larger than {} bytes", cap);
    }
    Ok(buf)
}

/// A stable id for a build, derived only from its inputs.
pub fn build_id(parts: &[&str]) -> String {
    sha256_bytes(parts.join("\0").as_bytes())
}

/// The default executable name used in packaged builds.
pub fn default_executable() -> String {
    format!("game/{}", executable_name())
}

fn looks_prepared(path: &Path) -> bool {
    path.is_dir()
        && (path.join("horizon-resources.json").is_file()
            || path.join("assets").is_dir()
            || path.join("data").is_dir())
}

/// Look in the usual places for Minecraft 26.3 resources prepared by the
/// official launcher.
pub fn discover_resources(instance: &Path) -> Option<PathBuf> {
    let mut candidates = vec![instance.join("resources")];
    if let Ok(p) = std::env::var("HORIZON_RESOURCES") {
        candidates.push(PathBuf::from(p));
    }
    if let Ok(p) = std::env::var("HORIZON_HOME") {
        candidates.push(PathBuf::from(p).join("resources"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(".local/share/horizon/resources"));
    }
    candidates.into_iter().find(|p| looks_prepared(p))
}
