//! Runtime loading from a plain "mods" folder.
//!
//! A `.hmmod` carries a complete prebuilt Horizon client, not a small runtime
//! plugin. "Loading all `.hmmod` files at runtime" therefore means finding a
//! build that already contains every installed mod, then starting that one
//! client. This module implements the discovery, grouping and source-level
//! combination needed for that workflow.

use crate::convert::{convert, ConvertOptions};
use crate::format::{sha256_bytes, sha256_file, HmmodManifest};
use crate::hnmod::{combined_source_sha, HnmodBundle};
use crate::store::{InstalledMod, LoaderHome};
use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

const MAX_SOURCE_BUNDLE: u64 = 128 * 1024 * 1024;

/// Every `.hmmod` directly inside `dir`, sorted by path.
pub fn find_hmmods(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !dir.is_dir() {
        anyhow::bail!("mod folder not found: {}", dir.display());
    }
    for entry in fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))? {
        let path = entry?.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("hmmod") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Install every `.hmmod` directly inside `dir` into the local store.
pub fn install_directory(home: &LoaderHome, dir: &Path) -> Result<Vec<InstalledMod>> {
    let archives = find_hmmods(dir)?;
    if archives.is_empty() {
        anyhow::bail!("no .hmmod files found in {}", dir.display());
    }
    let mut installed = Vec::new();
    for archive in archives {
        let item = home
            .install(&archive)
            .with_context(|| format!("cannot install {}", archive.display()))?;
        installed.push(item);
    }
    Ok(installed)
}

/// The mod ids a package provides. Older/hand-made packages may have an empty
/// `mods` list, in which case the package id itself is the provided mod.
pub fn provided_mod_ids(manifest: &HmmodManifest) -> BTreeSet<String> {
    let ids: BTreeSet<_> = manifest.mods.iter().map(|m| m.id.clone()).collect();
    if ids.is_empty() {
        BTreeSet::from([manifest.id.clone()])
    } else {
        ids
    }
}

/// Installed packages that share one prebuilt client (`build_id`).
#[derive(Clone, Debug)]
pub struct BuildGroup {
    pub build_id: String,
    pub packages: Vec<HmmodManifest>,
    pub mod_ids: BTreeSet<String>,
}

impl BuildGroup {
    pub fn representative(&self) -> &HmmodManifest {
        &self.packages[0]
    }

    pub fn describe(&self) -> String {
        let mods: Vec<_> = self.mod_ids.iter().cloned().collect();
        format!(
            "{} [{}]",
            &self.build_id[..12.min(self.build_id.len())],
            mods.join(", ")
        )
    }
}

/// What a set of installed packages can do together at runtime.
#[derive(Clone, Debug)]
pub struct RuntimePlan {
    pub all_mod_ids: BTreeSet<String>,
    pub groups: Vec<BuildGroup>,
    /// A package whose client already contains every installed mod.
    pub selected: Option<HmmodManifest>,
    /// Mods that no single installed build contains.
    pub missing: Vec<String>,
}

impl RuntimePlan {
    pub fn summary(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "{} installed package(s), {} mod id(s), {} build group(s)",
            self.groups.iter().map(|g| g.packages.len()).sum::<usize>(),
            self.all_mod_ids.len(),
            self.groups.len()
        ));
        for group in &self.groups {
            out.push_str(&format!("\n  build {}", group.describe()));
        }
        out
    }
}

/// Group manifests by build id and pick a build that covers every mod id.
pub fn plan(manifests: &[HmmodManifest]) -> RuntimePlan {
    let mut all_mod_ids = BTreeSet::new();
    let mut groups: BTreeMap<String, BuildGroup> = BTreeMap::new();

    for manifest in manifests {
        let ids = provided_mod_ids(manifest);
        all_mod_ids.extend(ids.iter().cloned());
        let group = groups
            .entry(manifest.build_id.clone())
            .or_insert_with(|| BuildGroup {
                build_id: manifest.build_id.clone(),
                packages: Vec::new(),
                mod_ids: BTreeSet::new(),
            });
        group.packages.push(manifest.clone());
        group.mod_ids.extend(ids);
    }

    let groups: Vec<_> = groups.into_values().collect();
    let selected_group = groups
        .iter()
        .find(|group| group.mod_ids.is_superset(&all_mod_ids))
        .cloned();
    let selected = selected_group
        .as_ref()
        .map(|group| group.representative().clone());
    let missing = match &selected_group {
        Some(group) => all_mod_ids.difference(&group.mod_ids).cloned().collect(),
        None => all_mod_ids.iter().cloned().collect(),
    };

    RuntimePlan {
        all_mod_ids,
        groups,
        selected,
        missing,
    }
}

/// Install a folder of packages (when given), then launch a build that covers
/// every installed mod. Unlike `launch`, this never asks the user to choose one
/// of several independently built clients: it either finds a complete build or
/// explains how to combine the packages.
pub fn load_directory(
    home: &LoaderHome,
    dir: Option<&Path>,
    resources: Option<&Path>,
    game_args: &[String],
    dry_run: bool,
) -> Result<()> {
    if let Some(dir) = dir {
        let installed = install_directory(home, dir)?;
        println!(
            "Imported {} package(s) from {}",
            installed.len(),
            dir.display()
        );
    }

    let mods = home.list()?;
    if mods.is_empty() {
        anyhow::bail!(
            "no .hmmod is installed in {}; put .hmmod files in the mod folder and run \
             `hmloader runtime`",
            home.mods_dir().display()
        );
    }
    let manifests: Vec<_> = mods.iter().map(|m| m.manifest.clone()).collect();
    let plan = plan(&manifests);
    println!("{}", plan.summary());

    let Some(selected) = plan.selected else {
        let groups = plan
            .groups
            .iter()
            .map(|g| format!("  build {}", g.describe()))
            .collect::<Vec<_>>()
            .join("\n");
        anyhow::bail!(
            "no installed .hmmod contains all {} mod(s), so they cannot be loaded into one game\n\
             installed builds:\n{groups}\n\
             A .hmmod is a complete prebuilt client; separately built clients cannot be \
             combined at runtime.\n\
             Build one package from the original `.hnmod` sources (every .hmmod keeps them \
             under source/):\n  \
             hmloader combine {} --out modpack.hmmod --source <horizon> --mixins <horizon-mixins>\n\
             then:\n  hmloader remove <old-id> && hmloader install modpack.hmmod",
            plan.all_mod_ids.len(),
            home.mods_dir().display()
        );
    };

    home.launch(Some(&selected.id), resources, game_args, dry_run)
}

/// Extract every `payload/source/*.hnmod` file from a package. Returns written
/// paths, named by SHA-256 so duplicate sources are stable.
pub fn extract_source_bundles(archive: &Path, dest: &Path) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(dest)?;
    let file =
        fs::File::open(archive).with_context(|| format!("cannot open {}", archive.display()))?;
    let mut zip = ZipArchive::new(file)
        .with_context(|| format!("{} is not a zip archive", archive.display()))?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let name = entry.name().to_string();
        if !name.starts_with("payload/source/") || !name.ends_with(".hnmod") {
            continue;
        }
        if entry.size() > MAX_SOURCE_BUNDLE {
            anyhow::bail!(
                "{} contains an unreasonably large source bundle: {}",
                archive.display(),
                name
            );
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        let sha = sha256_bytes(&bytes);
        let path = dest.join(format!("{sha}.hnmod"));
        if !path.exists() {
            fs::write(&path, bytes)?;
        }
        out.push(path);
    }
    Ok(out)
}

/// Options for `hmloader combine`.
#[derive(Clone, Debug)]
pub struct CombineOptions {
    pub dir: PathBuf,
    pub out: PathBuf,
    pub source: PathBuf,
    pub mixins: PathBuf,
    pub profile: String,
    pub target_dir: Option<PathBuf>,
    pub work_dir: PathBuf,
    pub offline: bool,
    pub mixins_cli: Option<PathBuf>,
    pub horizon_version: Option<String>,
    pub allow_mismatch: bool,
    pub id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
}

/// Combine all `.hmmod` files in a folder into one prebuilt `.hmmod` by
/// extracting the original `.hnmod` sources each package carries. This is the
/// author/build-time equivalent of Fabric loading several mod jars; it needs a
/// Rust toolchain because Horizon mods are compile-time source mixins.
pub fn combine(opts: &CombineOptions) -> Result<PathBuf> {
    let archives = find_hmmods(&opts.dir)?;
    if archives.is_empty() {
        anyhow::bail!("no .hmmod files found in {}", opts.dir.display());
    }
    fs::create_dir_all(&opts.work_dir)?;
    let source_dir = opts.work_dir.join("combined-sources");

    let mut seen = BTreeSet::new();
    let mut bundles = Vec::new();
    for archive in &archives {
        let sources = extract_source_bundles(archive, &source_dir)
            .with_context(|| format!("cannot read sources from {}", archive.display()))?;
        if sources.is_empty() {
            anyhow::bail!(
                "{} carries no source/*.hnmod; re-create it with a current hmloader convert",
                archive.display()
            );
        }
        for path in sources {
            let sha = sha256_file(&path)?;
            if seen.insert(sha) {
                bundles.push(path);
            }
        }
    }
    bundles.sort();
    if bundles.len() < 2 {
        eprintln!(
            "==> only one unique source bundle found; the combined package will be the same build"
        );
    }

    let mut opened = Vec::new();
    for path in &bundles {
        opened.push(HnmodBundle::open(path)?);
    }
    let source_sha = combined_source_sha(&opened);
    let id = opts
        .id
        .clone()
        .unwrap_or_else(|| format!("modpack_{}", &source_sha[..8.min(source_sha.len())]));
    let name = opts
        .name
        .clone()
        .unwrap_or_else(|| "Combined modpack".to_string());
    let version = opts.version.clone().unwrap_or_else(|| "1.0.0".to_string());

    eprintln!(
        "==> combining {} unique .hnmod source bundle(s) into {}",
        bundles.len(),
        opts.out.display()
    );
    let convert_opts = ConvertOptions {
        bundles,
        source: opts.source.clone(),
        mixins: opts.mixins.clone(),
        out: opts.out.clone(),
        profile: opts.profile.clone(),
        target_dir: opts.target_dir.clone(),
        work_dir: opts.work_dir.clone(),
        offline: opts.offline,
        mixins_cli: opts.mixins_cli.clone(),
        horizon_version: opts.horizon_version.clone(),
        allow_mismatch: opts.allow_mismatch,
        prebuilt_binary: None,
        id: Some(id),
        name: Some(name),
        version: Some(version),
    };
    convert(&convert_opts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::ModEntry;

    fn manifest(id: &str, build: &str, mods: &[(&str, &str)]) -> HmmodManifest {
        HmmodManifest {
            format: 1,
            kind: "prebuilt-game".into(),
            id: id.into(),
            name: id.into(),
            version: "1.0.0".into(),
            horizon_revision: "ba6344a59cb9e755b730dcf02285fc0d33b97d3c".into(),
            horizon_version: None,
            target: "x86_64-unknown-linux-gnu".into(),
            profile: "fast".into(),
            build_id: build.into(),
            executable: "game/horizon-client".into(),
            source_sha256: None,
            source_bundle: None,
            mods: mods
                .iter()
                .map(|(id, version)| ModEntry {
                    id: (*id).into(),
                    version: (*version).into(),
                    name: None,
                })
                .collect(),
            files: Default::default(),
            notes: None,
        }
    }

    #[test]
    fn one_build_with_all_mods_is_selected() {
        let ms = vec![
            manifest("a", "build-a", &[("a", "1.0.0")]),
            manifest("b", "build-b", &[("b", "1.0.0")]),
            manifest("pack", "build-pack", &[("a", "1.0.0"), ("b", "1.0.0")]),
        ];
        let p = plan(&ms);
        assert_eq!(p.all_mod_ids.len(), 2);
        assert_eq!(p.selected.as_ref().unwrap().id, "pack");
        assert!(p.missing.is_empty());
    }

    #[test]
    fn separate_builds_are_not_silently_merged() {
        let ms = vec![
            manifest("a", "build-a", &[("a", "1.0.0")]),
            manifest("b", "build-b", &[("b", "1.0.0")]),
        ];
        let p = plan(&ms);
        assert!(p.selected.is_none());
        assert_eq!(p.missing, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(p.groups.len(), 2);
    }

    #[test]
    fn packages_with_the_same_build_share_one_group() {
        let ms = vec![
            manifest("a", "same-build", &[("a", "1.0.0")]),
            manifest("b", "same-build", &[("b", "1.0.0")]),
        ];
        let p = plan(&ms);
        assert_eq!(p.groups.len(), 1);
        assert_eq!(p.selected.as_ref().unwrap().id, "a");
        assert!(p.missing.is_empty());
    }

    #[test]
    fn empty_mods_list_falls_back_to_package_id() {
        let p = plan(&[manifest("solo", "build-solo", &[])]);
        assert!(p.all_mod_ids.contains("solo"));
        assert_eq!(p.selected.unwrap().id, "solo");
    }
}
