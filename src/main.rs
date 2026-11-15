use anyhow::Result;
use clap::{Parser, Subcommand};
use hmloader::convert::{self, ConvertOptions};
use hmloader::format::current_target;
use hmloader::store::LoaderHome;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "hmloader",
    version,
    about = "Precompiled .hmmod mod loader for Horizon (Minecraft: Java Edition 26.3 in Rust)",
    long_about = "Horizon's .hnmod files are compiled on every player's computer. hmloader installs .hmmod \
files, which already contain the compiled Horizon client, and starts it without a Rust build."
)]
struct Cli {
    /// Data folder for installed mods and instances.
    #[arg(long, global = true, env = "HORIZON_HMMOD_HOME")]
    home: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show what an .hmmod contains, without installing it.
    Info { archive: PathBuf },
    /// Verify, unpack and add an .hmmod to the mods folder.
    Install {
        archive: PathBuf,
        /// Skip the post-install verification pass.
        #[arg(long)]
        no_verify: bool,
    },
    /// List installed mods.
    List,
    /// Remove an installed mod and its unpacked files.
    Remove { id: String },
    /// Check hashes, target and the unpacked client of an installed mod.
    Verify { id: String },
    /// Launch the prebuilt Horizon client of an installed mod.
    Launch {
        /// Mod id. Optional when exactly one mod (or one build group) is installed.
        id: Option<String>,
        /// Prepared Horizon resources (the folder holding horizon-resources.json).
        #[arg(long)]
        resources: Option<PathBuf>,
        /// Print the command instead of starting the game.
        #[arg(long)]
        dry_run: bool,
        /// Arguments passed to horizon-client.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Convert one or more .hnmod bundles into one precompiled .hmmod.
    Convert {
        /// .hnmod files. Several are compiled together into one game, like a modpack.
        bundles: Vec<PathBuf>,
        #[arg(long)]
        out: PathBuf,
        /// Horizon source checkout (git repository) containing the pinned commit.
        #[arg(long)]
        source: PathBuf,
        /// horizon-mixins checkout (git repository).
        #[arg(long)]
        mixins: PathBuf,
        /// Cargo profile: fast (Horizon's performance profile) or release.
        #[arg(long, default_value = "fast")]
        profile: String,
        /// Reuse a Cargo target directory (falls back to the staged workspace).
        #[arg(long)]
        target_dir: Option<PathBuf>,
        /// Where temporary sources and the staged workspace live.
        #[arg(long)]
        work_dir: Option<PathBuf>,
        /// Do not let Cargo access the network; dependencies must be cached.
        #[arg(long)]
        offline: bool,
        /// Path to an already-built horizon-mixins binary.
        #[arg(long)]
        mixins_cli: Option<PathBuf>,
        /// Human-readable base version to record in the manifest.
        #[arg(long)]
        horizon_version: Option<String>,
        /// Build bundles that pin different Horizon revisions anyway.
        #[arg(long)]
        allow_mismatch: bool,
        /// Skip compilation and package this client binary instead.
        #[arg(long)]
        prebuilt_binary: Option<PathBuf>,
    },
    /// Package an already-built horizon-client as an .hmmod (advanced/testing).
    Pack {
        #[arg(long)]
        binary: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        horizon_revision: String,
        #[arg(long)]
        horizon_version: Option<String>,
        #[arg(long, default_value = "fast")]
        profile: String,
        #[arg(long)]
        source_bundle: Option<PathBuf>,
    },
    /// Ask the official Horizon launcher to import a precompiled client as a version.
    ImportLauncher {
        id: String,
        /// Path to the official horizon-launcher executable.
        #[arg(long)]
        launcher: PathBuf,
        /// Installed launcher version whose prepared resources the build reuses.
        #[arg(long)]
        version: Option<String>,
    },
    /// Check the environment and installed mods.
    Doctor {
        /// Optional official launcher path to check as well.
        #[arg(long)]
        launcher: Option<PathBuf>,
    },
}

fn home(cli: &Cli) -> LoaderHome {
    match &cli.home {
        Some(p) => LoaderHome::new(p),
        None => LoaderHome::default_home(),
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let store = home(&cli);
    match cli.command {
        Commands::Info { archive } => {
            let manifest = store.inspect(&archive)?;
            println!("{}", serde_json::to_string_pretty(&manifest)?);
            println!("\narchive: {}", archive.display());
            println!("size:    {} bytes", std::fs::metadata(&archive)?.len());
        }
        Commands::Install { archive, no_verify } => {
            let installed = store.install(&archive)?;
            println!(
                "Installed {} {} ({}) -> {}",
                installed.manifest.name,
                installed.manifest.version,
                installed.manifest.id,
                installed.archive.display()
            );
            println!(
                "Build {} ({} mods)",
                &installed.manifest.build_id[..12],
                installed.manifest.mods.len()
            );
            if !no_verify {
                let checked = store.verify(&installed.manifest.id)?;
                println!(
                    "Verified {} files in {}",
                    checked.manifest.files.len(),
                    checked.installed_dir.display()
                );
            }
            println!("Start it with: hmloader launch {}", installed.manifest.id);
        }
        Commands::List => {
            let mods = store.list()?;
            if mods.is_empty() {
                println!("No .hmmod installed in {}", store.mods_dir().display());
                return Ok(());
            }
            println!(
                "{:<20} {:<12} {:<28} {:<20} MODS",
                "ID", "VERSION", "BUILD", "HORIZON"
            );
            for m in mods {
                let md = &m.manifest;
                let mods: Vec<_> = md
                    .mods
                    .iter()
                    .map(|x| format!("{} {}", x.id, x.version))
                    .collect();
                println!(
                    "{:<20} {:<12} {:<28} {:<20} {}",
                    md.id,
                    md.version,
                    &md.build_id[..12.min(md.build_id.len())],
                    &md.horizon_revision[..12.min(md.horizon_revision.len())],
                    mods.join(", ")
                );
            }
        }
        Commands::Remove { id } => {
            let removed = store.remove(&id)?;
            for p in removed {
                println!("removed {}", p.display());
            }
        }
        Commands::Verify { id } => {
            let m = store.verify(&id)?;
            println!(
                "OK: {} {} ({} files, target {}, Horizon {})",
                m.manifest.id,
                m.manifest.version,
                m.manifest.files.len(),
                m.manifest.target,
                m.manifest.horizon_revision
            );
        }
        Commands::Launch {
            id,
            resources,
            dry_run,
            args,
        } => {
            store.launch(id.as_deref(), resources.as_deref(), &args, dry_run)?;
        }
        Commands::Convert {
            bundles,
            out,
            source,
            mixins,
            profile,
            target_dir,
            work_dir,
            offline,
            mixins_cli,
            horizon_version,
            allow_mismatch,
            prebuilt_binary,
        } => {
            let work_dir = work_dir.unwrap_or_else(|| store.work_dir());
            let opts = ConvertOptions {
                bundles,
                source,
                mixins,
                out,
                profile,
                target_dir,
                work_dir,
                offline,
                mixins_cli,
                horizon_version,
                allow_mismatch,
                prebuilt_binary,
            };
            convert::convert(&opts)?;
        }
        Commands::Pack {
            binary,
            out,
            id,
            name,
            version,
            horizon_revision,
            horizon_version,
            profile,
            source_bundle,
        } => {
            convert::pack_prebuilt(
                &out,
                &binary,
                &id,
                &name,
                &version,
                &horizon_revision,
                horizon_version.as_deref(),
                &profile,
                source_bundle.as_deref(),
            )?;
            println!("wrote {}", out.display());
        }
        Commands::ImportLauncher {
            id,
            launcher,
            version,
        } => {
            store.import_into_launcher(&id, &launcher, version.as_deref())?;
        }
        Commands::Doctor { launcher } => doctor(&store, launcher.as_deref())?,
    }
    Ok(())
}

fn doctor(store: &LoaderHome, launcher: Option<&Path>) -> Result<()> {
    println!("hmloader {}", env!("CARGO_PKG_VERSION"));
    println!("home:    {}", store.root.display());
    println!("mods:    {}", store.mods_dir().display());
    println!("target:  {}", current_target());
    println!("rust:    {}", option_version("rustc"));
    println!("cargo:   {}", option_version("cargo"));
    println!("git:     {}", option_version("git"));
    println!("tar:     {}", option_version("tar"));
    if let Some(launcher) = launcher {
        println!("launcher: {}", launcher.display());
        match std::process::Command::new(launcher)
            .arg("--version")
            .output()
        {
            Ok(o) if o.status.success() => {
                println!(
                    "launcher version: {}",
                    String::from_utf8_lossy(&o.stdout).trim()
                )
            }
            _ => println!("launcher: not runnable"),
        }
    }
    let mods = store.list()?;
    println!("installed: {} mod(s)", mods.len());
    for m in mods {
        let exe = m.installed_dir.join(&m.manifest.executable);
        let status = if exe.is_file() {
            "client present"
        } else {
            "CLIENT MISSING"
        };
        println!(
            "  {} {} -> {} ({status})",
            m.manifest.id,
            m.manifest.version,
            m.archive.display()
        );
        if m.manifest.target != current_target() {
            println!(
                "    WARNING: built for {}, this computer is {}",
                m.manifest.target,
                current_target()
            );
        }
    }
    if let Some(r) = std::env::var_os("HORIZON_HMMOD_HOME") {
        println!("HORIZON_HMMOD_HOME={}", r.to_string_lossy());
    }
    Ok(())
}

fn option_version(cmd: &str) -> String {
    std::process::Command::new(cmd)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .to_string()
        })
        .unwrap_or_else(|| "not found".to_string())
}
