//! `.hnmod` -> `.hmmod` conversion: stage the pinned Horizon source with
//! horizon-mixins, compile the client once, and package the result.

use crate::format::{
    current_target, executable_name, pack_hmmod, sha256_bytes, tree_files, PackOptions,
    FORMAT_VERSION,
};
use crate::hnmod::{combined_source_sha, HnmodBundle};
use crate::store::build_id as make_build_id;
use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Debug)]
pub struct ConvertOptions {
    pub bundles: Vec<PathBuf>,
    pub source: PathBuf,
    pub mixins: PathBuf,
    pub out: PathBuf,
    pub profile: String,
    pub target_dir: Option<PathBuf>,
    pub work_dir: PathBuf,
    pub offline: bool,
    pub mixins_cli: Option<PathBuf>,
    pub horizon_version: Option<String>,
    pub allow_mismatch: bool,
    /// Skip the compile and use this already-built client (for tests or when
    /// re-packaging an existing build).
    pub prebuilt_binary: Option<PathBuf>,
}

#[derive(Serialize)]
struct NativeModsJson<'a> {
    base: String,
    horizon: &'a str,
    profile: &'a str,
    mods: Vec<NativeModJson<'a>>,
    hmmod: bool,
}

#[derive(Serialize)]
struct NativeModJson<'a> {
    id: &'a str,
    version: &'a str,
}

fn resolve_target_dir(target_dir: Option<&Path>) -> Result<Option<PathBuf>> {
    Ok(match target_dir {
        Some(td) if td.is_absolute() => Some(td.to_path_buf()),
        Some(td) => Some(std::env::current_dir()?.join(td)),
        None => None,
    })
}

fn run(mut cmd: Command, what: &str) -> Result<()> {
    eprintln!("==> {what}");
    let status = cmd
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("cannot run {what}"))?;
    if !status.success() {
        anyhow::bail!("{what} failed with {status}");
    }
    Ok(())
}

fn find_mixins_cli(opts: &ConvertOptions) -> Result<PathBuf> {
    if let Some(path) = &opts.mixins_cli {
        return Ok(path.clone());
    }
    for name in ["horizon-mixins", "horizon-mixins.exe"] {
        let p = opts.mixins.join("target/release").join(name);
        if p.is_file() {
            return Ok(p);
        }
        let p = opts.mixins.join("target/debug").join(name);
        if p.is_file() {
            return Ok(p);
        }
    }
    eprintln!("==> building horizon-mixins (one time)");
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&opts.mixins)
        .args(["build", "--locked", "-p", "horizon-mixins-cli"]);
    if opts.offline {
        cmd.arg("--offline");
    }
    run(cmd, "cargo build horizon-mixins")?;
    for name in ["horizon-mixins", "horizon-mixins.exe"] {
        let p = opts.mixins.join("target/debug").join(name);
        if p.is_file() {
            return Ok(p);
        }
    }
    anyhow::bail!(
        "built horizon-mixins but cannot find its binary under {}",
        opts.mixins.join("target").display()
    )
}

fn export_source(source: &Path, revision: &str, workspace: &Path) -> Result<()> {
    if workspace.exists() {
        let marker = workspace.join(".horizon-revision");
        let same = fs::read_to_string(&marker)
            .map(|s| s.trim() == revision)
            .unwrap_or(false);
        if same {
            return Ok(());
        }
        fs::remove_dir_all(workspace)?;
    }
    fs::create_dir_all(workspace)?;
    let archive = fs::canonicalize(workspace)?.with_extension("tar");
    eprintln!("==> exporting Horizon {revision} from {}", source.display());
    let mut cmd = Command::new("git");
    cmd.current_dir(source).args([
        "archive",
        "--format=tar",
        &format!("--output={}", archive.display()),
        revision,
    ]);
    run(cmd, "git archive")?;
    let mut cmd = Command::new("tar");
    cmd.current_dir(workspace).arg("-xf").arg(&archive);
    run(cmd, "tar extract")?;
    fs::remove_file(&archive).ok();
    // git archive can contain the repository's documentation symlinks. The
    // mixin tool deliberately refuses symlinks, so materialize the few that
    // are regular files pointing inside the tree (for example CLAUDE.md).
    let files: Vec<_> = walkdir::WalkDir::new(workspace)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_symlink())
        .map(|e| e.path().to_path_buf())
        .collect();
    for link in files {
        let target = fs::read_link(&link)?;
        let resolved = if target.is_absolute() {
            target
        } else {
            link.parent().unwrap_or(workspace).join(target)
        };
        let canonical_root = fs::canonicalize(workspace)?;
        let canonical_target = fs::canonicalize(&resolved)
            .with_context(|| format!("broken symlink in source export: {}", link.display()))?;
        if !canonical_target.starts_with(&canonical_root) || !canonical_target.is_file() {
            anyhow::bail!(
                "refusing unsupported symlink in source export: {}",
                link.display()
            );
        }
        let bytes = fs::read(&canonical_target)?;
        fs::remove_file(&link)?;
        fs::write(&link, bytes)?;
    }
    fs::write(workspace.join(".horizon-revision"), format!("{revision}\n"))?;
    Ok(())
}

fn touch_tree(root: &Path) -> Result<()> {
    use std::time::SystemTime;
    let now = SystemTime::now();
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
    {
        if entry.file_type().is_file() {
            if let Ok(f) = fs::OpenOptions::new().write(true).open(entry.path()) {
                let times = fs::FileTimes::new().set_modified(now);
                let _ = f.set_times(times);
            }
        }
    }
    Ok(())
}

/// Convert one or more `.hnmod` bundles into a single precompiled `.hmmod`.
pub fn convert(opts: &ConvertOptions) -> Result<PathBuf> {
    if opts.bundles.is_empty() {
        anyhow::bail!("at least one .hnmod is required");
    }
    if !opts.source.is_dir() {
        anyhow::bail!(
            "Horizon source checkout not found: {}",
            opts.source.display()
        );
    }
    if !opts.mixins.is_dir() {
        anyhow::bail!(
            "horizon-mixins checkout not found: {}",
            opts.mixins.display()
        );
    }
    fs::create_dir_all(&opts.work_dir)?;

    let mut bundles = Vec::new();
    for path in &opts.bundles {
        bundles.push(HnmodBundle::open(path)?);
    }
    let revision = bundles[0].manifest.horizon.clone();
    if !opts.allow_mismatch {
        for b in &bundles[1..] {
            if b.manifest.horizon != revision {
                anyhow::bail!(
                    "{} pins Horizon {}, but {} pins {}; build them separately or pass --allow-mismatch",
                    bundles[0].path.display(),
                    revision,
                    b.path.display(),
                    b.manifest.horizon
                );
            }
        }
    }
    let mut ids = std::collections::BTreeSet::new();
    for b in &bundles {
        if !ids.insert(b.manifest.id.clone()) {
            anyhow::bail!("two bundles have the same mod id: {}", b.manifest.id);
        }
    }

    let target = current_target().to_string();
    let source_sha = combined_source_sha(&bundles);
    let build_id = make_build_id(&[
        "hmmod-format-1",
        &revision,
        &source_sha,
        &opts.profile,
        &target,
    ]);
    let short = &build_id[..16];
    let work = opts.work_dir.join(short);
    fs::create_dir_all(&work)?;
    let workspace = work.join("workspace");
    let mods_dir = work.join("mods");
    let stage = work.join("stage");
    let package = work.join("package");

    export_source(&opts.source, &revision, &workspace)?;

    if mods_dir.exists() {
        fs::remove_dir_all(&mods_dir)?;
    }
    fs::create_dir_all(&mods_dir)?;
    for b in &bundles {
        b.extract_bundle(&mods_dir.join(&b.manifest.id))?;
    }
    touch_tree(&workspace)?;

    let cli = find_mixins_cli(opts)?;
    eprintln!("==> applying mixins into {}", stage.display());
    let mut cmd = Command::new(&cli);
    cmd.args(["apply", "--workspace"])
        .arg(&workspace)
        .arg("--mods")
        .arg(&mods_dir)
        .arg("--out")
        .arg(&stage);
    run(cmd, "horizon-mixins apply")?;
    touch_tree(&stage)?;

    let binary = if let Some(prebuilt) = &opts.prebuilt_binary {
        prebuilt.clone()
    } else {
        if !opts.offline {
            let mut fetch = Command::new("cargo");
            fetch
                .arg("fetch")
                .args(["--locked", "--manifest-path"])
                .arg(stage.join("Cargo.toml"));
            run(fetch, "cargo fetch")?;
        }
        // Cargo runs inside `stage`, so a relative --target-dir would otherwise
        // be resolved there while the artifact lookup below used the caller's
        // working directory. Pin both to the same absolute path.
        let target_dir = resolve_target_dir(opts.target_dir.as_deref())?;
        let mut build = Command::new("cargo");
        build.current_dir(&stage).args([
            "build",
            "--locked",
            "-p",
            "horizon-client",
            "--bin",
            "horizon-client",
            "--no-default-features",
            "--profile",
            &opts.profile,
        ]);
        if opts.offline {
            build.arg("--offline");
        }
        if let Some(td) = &target_dir {
            build.arg("--target-dir").arg(td);
        }
        run(build, "cargo build horizon-client")?;
        let target_dir = target_dir.unwrap_or_else(|| stage.join("target"));
        let candidate = target_dir.join(&opts.profile).join(executable_name());
        if !candidate.is_file() {
            anyhow::bail!(
                "cargo finished but {} was not produced (check the profile name)",
                candidate.display()
            );
        }
        candidate
    };
    let binary_size = fs::metadata(&binary)?.len();
    if binary_size < 1024 * 1024 {
        anyhow::bail!(
            "{} is only {} bytes; that does not look like a Horizon client",
            binary.display(),
            binary_size
        );
    }

    if package.exists() {
        fs::remove_dir_all(&package)?;
    }
    fs::create_dir_all(package.join("game"))?;
    fs::create_dir_all(package.join("source"))?;
    let exe_rel = format!("game/{}", executable_name());
    fs::copy(&binary, package.join(&exe_rel))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(package.join(&exe_rel), fs::Permissions::from_mode(0o755))?;
    }

    let base = opts
        .horizon_version
        .clone()
        .or_else(|| bundles[0].manifest.game.clone())
        .unwrap_or_else(|| format!("Horizon {}", &revision[..7.min(revision.len())]));
    let native_mods = NativeModsJson {
        base,
        horizon: &revision,
        profile: &opts.profile,
        mods: bundles
            .iter()
            .map(|b| NativeModJson {
                id: &b.manifest.id,
                version: &b.manifest.version,
            })
            .collect(),
        hmmod: true,
    };
    fs::write(
        package.join("game/native-mods.json"),
        serde_json::to_vec_pretty(&native_mods)?,
    )?;
    for b in &bundles {
        let name = b
            .path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("bundle.hnmod");
        fs::copy(&b.path, package.join("source").join(name))?;
    }

    let mut manifest = bundles[0].manifest_base(&opts.profile, &target, &build_id);
    manifest.horizon_version = Some(native_mods.base.clone());
    manifest.mods = bundles.iter().map(|b| b.mod_entry()).collect();
    manifest.source_sha256 = Some(source_sha);
    manifest.build_id = build_id.clone();
    manifest.notes = Some(format!(
        "Precompiled Horizon client for {} mod(s). Built from Horizon {} with profile {}.",
        bundles.len(),
        revision,
        opts.profile
    ));

    let files: Vec<(PathBuf, String)> = tree_files(&package)?;
    eprintln!("==> packing {}", opts.out.display());
    let manifest = pack_hmmod(
        &opts.out,
        PackOptions {
            manifest_base: manifest,
            payload_files: files,
        },
    )?;
    let size = fs::metadata(&opts.out)?.len();
    eprintln!(
        "==> wrote {} ({} bytes, build {}, executable {} bytes)",
        opts.out.display(),
        size,
        &manifest.build_id[..12],
        binary_size
    );
    Ok(opts.out.clone())
}

/// A tiny manifest for `pack` from an already-built client.
#[allow(clippy::too_many_arguments)]
pub fn pack_prebuilt(
    out: &Path,
    binary: &Path,
    id: &str,
    name: &str,
    version: &str,
    horizon_revision: &str,
    horizon_version: Option<&str>,
    profile: &str,
    source_bundle: Option<&Path>,
) -> Result<PathBuf> {
    let stage = tempfile::tempdir()?;
    let pkg = stage.path();
    fs::create_dir_all(pkg.join("game"))?;
    let exe_rel = format!("game/{}", executable_name());
    fs::copy(binary, pkg.join(&exe_rel))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(pkg.join(&exe_rel), fs::Permissions::from_mode(0o755))?;
    }
    let native_mods = serde_json::json!({
        "base": horizon_version.unwrap_or("prebuilt"),
        "horizon": horizon_revision,
        "profile": profile,
        "mods": [{"id": id, "version": version}],
        "hmmod": true,
    });
    fs::write(
        pkg.join("game/native-mods.json"),
        serde_json::to_vec_pretty(&native_mods)?,
    )?;
    if let Some(src) = source_bundle {
        let dir = pkg.join("source");
        fs::create_dir_all(&dir)?;
        let name = src
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("bundle.hnmod");
        fs::copy(src, dir.join(name))?;
    }
    let files = tree_files(pkg)?;
    let target = current_target().to_string();
    let source_sha = source_bundle.map(crate::format::sha256_file).transpose()?;
    let build_id = sha256_bytes(
        format!(
            "{id}\0{version}\0{horizon_revision}\0{profile}\0{target}\0{}",
            source_sha.as_deref().unwrap_or("")
        )
        .as_bytes(),
    );
    let manifest = crate::format::HmmodManifest {
        format: FORMAT_VERSION,
        kind: "prebuilt-game".into(),
        id: id.into(),
        name: name.into(),
        version: version.into(),
        horizon_revision: horizon_revision.into(),
        horizon_version: horizon_version.map(str::to_string),
        target,
        profile: profile.into(),
        build_id,
        executable: exe_rel,
        source_sha256: source_sha,
        source_bundle: source_bundle
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .map(str::to_string),
        mods: vec![crate::format::ModEntry {
            id: id.into(),
            version: version.into(),
            name: Some(name.into()),
        }],
        files: Default::default(),
        notes: Some("Packed from an existing prebuilt Horizon client.".into()),
    };
    pack_hmmod(
        out,
        PackOptions {
            manifest_base: manifest,
            payload_files: files,
        },
    )?;
    Ok(out.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::resolve_target_dir;
    use std::path::PathBuf;

    #[test]
    fn relative_target_dir_is_resolved_from_the_caller_working_directory() {
        let relative = PathBuf::from("build/relative-target");
        let resolved = resolve_target_dir(Some(&relative)).unwrap().unwrap();
        assert!(resolved.is_absolute());
        assert!(resolved.ends_with(&relative));
    }

    #[test]
    fn absolute_target_dir_is_kept_as_is() {
        let absolute = std::env::temp_dir().join("hmloader-absolute-target");
        let resolved = resolve_target_dir(Some(&absolute)).unwrap().unwrap();
        assert_eq!(resolved, absolute);
    }

    #[test]
    fn no_target_dir_stays_none() {
        assert_eq!(resolve_target_dir(None).unwrap(), None);
    }
}
