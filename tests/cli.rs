use hmloader::convert::pack_prebuilt;
use hmloader::format::current_target;
use hmloader::hnmod::HnmodBundle;
use hmloader::store::LoaderHome;
use std::fs;
use std::io::Write;
use zip::write::FileOptions;

fn fake_binary(dir: &std::path::Path) -> std::path::PathBuf {
    let p = dir.join("horizon-client");
    let mut f = fs::File::create(&p).unwrap();
    writeln!(f, "#!/bin/sh").unwrap();
    writeln!(f, "echo FAKE-HORIZON \"$@\"").unwrap();
    drop(f);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    }
    p
}

#[test]
fn packs_installs_verifies_and_launches() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_binary(tmp.path());
    let archive = tmp.path().join("demo-0.1.0.hmmod");
    pack_prebuilt(
        &archive,
        &bin,
        "demo",
        "Demo",
        "0.1.0",
        "ba6344a59cb9e755b730dcf02285fc0d33b97d3c",
        Some("Minecraft 26.3"),
        "fast",
        None,
    )
    .unwrap();

    let home = LoaderHome::new(tmp.path().join("home"));
    let installed = home.install(&archive).unwrap();
    assert_eq!(installed.manifest.id, "demo");
    assert_eq!(installed.manifest.target, current_target());
    assert!(installed.archive.starts_with(home.mods_dir()));

    let listed = home.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].manifest.name, "Demo");

    let verified = home.verify("demo").unwrap();
    assert!(verified.manifest.files.len() >= 2);

    home.launch(Some("demo"), None, &["--frames".into(), "1".into()], false)
        .unwrap();
    assert!(home.instance_dir("demo").exists());
}

#[test]
fn detects_tampering_after_install() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_binary(tmp.path());
    let archive = tmp.path().join("demo.hmmod");
    pack_prebuilt(
        &archive,
        &bin,
        "demo",
        "Demo",
        "1.0.0",
        "ba6344a59cb9e755b730dcf02285fc0d33b97d3c",
        None,
        "fast",
        None,
    )
    .unwrap();
    let home = LoaderHome::new(tmp.path().join("home"));
    let installed = home.install(&archive).unwrap();
    let exe = installed.installed_dir.join(&installed.manifest.executable);
    fs::write(&exe, b"tampered").unwrap();
    let err = home.verify("demo").unwrap_err().to_string();
    assert!(err.contains("checksum mismatch"), "{err}");
}

#[test]
fn rejects_path_traversal_in_archive() {
    let tmp = tempfile::tempdir().unwrap();
    let archive = tmp.path().join("bad.hmmod");
    let file = fs::File::create(&archive).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = FileOptions::default();
    zip.start_file("hmmod.json", opts).unwrap();
    let manifest = serde_json::json!({
        "format": 1,
        "kind": "prebuilt-game",
        "id": "bad",
        "name": "Bad",
        "version": "1.0.0",
        "horizon_revision": "ba6344a59cb9e755b730dcf02285fc0d33b97d3c",
        "target": current_target(),
        "profile": "fast",
        "build_id": "deadbeef",
        "executable": "game/horizon-client",
        "files": {"game/horizon-client": "00", "../escape": "00"},
        "mods": []
    });
    zip.write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
        .unwrap();
    zip.start_file("payload/game/horizon-client", opts).unwrap();
    zip.write_all(b"x").unwrap();
    zip.start_file("payload/../escape", opts).unwrap();
    zip.write_all(b"x").unwrap();
    zip.finish().unwrap();

    let out = tmp.path().join("out");
    let err = hmloader::format::extract_hmmod(&archive, &out)
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("safety") || err.contains("unsafe") || err.contains("outside"),
        "{err}"
    );
    assert!(!tmp.path().join("escape").exists());
}

#[test]
fn reads_the_test_hnmod_fixture() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/example_mod-0.1.0.hnmod");
    let bundle = HnmodBundle::open(&path).unwrap();
    assert_eq!(bundle.manifest.id, "example_mod");
    assert_eq!(bundle.manifest.version, "0.1.0");
    assert_eq!(
        bundle.manifest.horizon,
        "ba6344a59cb9e755b730dcf02285fc0d33b97d3c"
    );
    let dest = tempfile::tempdir().unwrap();
    bundle.extract_bundle(dest.path()).unwrap();
    assert!(dest.path().join("horizon-mixin.toml").is_file());
    assert!(dest.path().join("break.rs").is_file());
}

#[test]
fn cli_pack_install_list_and_dry_run() {
    let exe = env!("CARGO_BIN_EXE_hmloader");
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_binary(tmp.path());
    let archive = tmp.path().join("demo.hmmod");
    let status = std::process::Command::new(exe)
        .arg("pack")
        .arg("--binary")
        .arg(&bin)
        .arg("--out")
        .arg(&archive)
        .arg("--id")
        .arg("demo")
        .arg("--name")
        .arg("Demo")
        .arg("--version")
        .arg("0.1.0")
        .arg("--horizon-revision")
        .arg("ba6344a59cb9e755b730dcf02285fc0d33b97d3c")
        .arg("--source-bundle")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/example_mod-0.1.0.hnmod"),
        )
        .status()
        .unwrap();
    assert!(status.success());
    let home = tmp.path().join("home");
    let status = std::process::Command::new(exe)
        .arg("--home")
        .arg(&home)
        .arg("install")
        .arg(&archive)
        .status()
        .unwrap();
    assert!(status.success());
    let out = std::process::Command::new(exe)
        .arg("--home")
        .arg(&home)
        .arg("list")
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("demo"), "{text}");
    let out = std::process::Command::new(exe)
        .arg("--home")
        .arg(&home)
        .arg("launch")
        .arg("demo")
        .arg("--dry-run")
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("command:"), "{text}");
}
