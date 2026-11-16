use hmloader::convert::pack_prebuilt;
use hmloader::format::{
    current_target, pack_hmmod, sha256_file, tree_files, HmmodManifest, ModEntry, PackOptions,
};
use hmloader::runtime::{extract_source_bundles, install_directory, load_directory};
use hmloader::store::LoaderHome;
use std::fs;
use std::io::Write;
use std::path::Path;

fn fake_binary(dir: &Path) -> std::path::PathBuf {
    let p = dir.join("horizon-client");
    let mut f = fs::File::create(&p).unwrap();
    writeln!(f, "#!/bin/sh").unwrap();
    writeln!(f, "echo FAKE-RUNTIME \"$@\"").unwrap();
    drop(f);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    }
    p
}

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/example_mod-0.1.0.hnmod")
}

fn pack_demo(archive: &Path, bin: &Path, id: &str) {
    pack_prebuilt(
        archive,
        bin,
        id,
        id,
        "0.1.0",
        "ba6344a59cb9e755b730dcf02285fc0d33b97d3c",
        Some("Minecraft 26.3"),
        "fast",
        Some(&fixture()),
    )
    .unwrap();
}

fn pack_combined(archive: &Path, bin: &Path) {
    let tmp = tempfile::tempdir().unwrap();
    let pkg = tmp.path();
    fs::create_dir_all(pkg.join("game")).unwrap();
    let exe_rel = "game/horizon-client".to_string();
    fs::copy(bin, pkg.join(&exe_rel)).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(pkg.join(&exe_rel), fs::Permissions::from_mode(0o755)).unwrap();
    }
    fs::write(
        pkg.join("game/native-mods.json"),
        br#"{"mods":[{"id":"demo_a","version":"0.1.0"},{"id":"demo_b","version":"0.1.0"}]}"#,
    )
    .unwrap();
    let files = tree_files(pkg).unwrap();
    let manifest = HmmodManifest {
        format: 1,
        kind: "prebuilt-game".into(),
        id: "modpack".into(),
        name: "Modpack".into(),
        version: "1.0.0".into(),
        horizon_revision: "ba6344a59cb9e755b730dcf02285fc0d33b97d3c".into(),
        horizon_version: Some("Minecraft 26.3".into()),
        target: current_target().into(),
        profile: "fast".into(),
        build_id: "combined-build".into(),
        executable: exe_rel,
        source_sha256: None,
        source_bundle: None,
        mods: vec![
            ModEntry {
                id: "demo_a".into(),
                version: "0.1.0".into(),
                name: None,
            },
            ModEntry {
                id: "demo_b".into(),
                version: "0.1.0".into(),
                name: None,
            },
        ],
        files: Default::default(),
        notes: None,
    };
    pack_hmmod(
        archive,
        PackOptions {
            manifest_base: manifest,
            payload_files: files,
        },
    )
    .unwrap();
}

#[test]
fn install_dir_imports_every_package() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_binary(tmp.path());
    let input = tmp.path().join("downloads");
    fs::create_dir_all(&input).unwrap();
    pack_demo(&input.join("a.hmmod"), &bin, "demo_a");
    pack_demo(&input.join("b.hmmod"), &bin, "demo_b");

    let home = LoaderHome::new(tmp.path().join("home"));
    let installed = install_directory(&home, &input).unwrap();
    assert_eq!(installed.len(), 2);
    assert_eq!(home.list().unwrap().len(), 2);
}

#[test]
fn runtime_rejects_separate_builds_but_launches_a_complete_one() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_binary(tmp.path());
    let input = tmp.path().join("downloads");
    fs::create_dir_all(&input).unwrap();
    pack_demo(&input.join("a.hmmod"), &bin, "demo_a");
    pack_demo(&input.join("b.hmmod"), &bin, "demo_b");

    let home = LoaderHome::new(tmp.path().join("home"));
    let err = load_directory(&home, Some(&input), None, &[], true)
        .unwrap_err()
        .to_string();
    assert!(err.contains("cannot be loaded into one game"), "{err}");
    assert!(err.contains("hmloader combine"), "{err}");

    pack_combined(&input.join("pack.hmmod"), &bin);
    let home = LoaderHome::new(tmp.path().join("home2"));
    load_directory(&home, Some(&input), None, &[], true).unwrap();
    assert_eq!(home.list().unwrap().len(), 3);

    let home = LoaderHome::new(tmp.path().join("home3"));
    install_directory(&home, &input).unwrap();
    fs::remove_file(home.mods_dir().join("demo_a-0.1.0.hmmod")).unwrap();
    fs::remove_file(home.mods_dir().join("demo_b-0.1.0.hmmod")).unwrap();
    load_directory(&home, None, None, &[], true).unwrap();
}

#[test]
fn source_bundles_are_extracted_from_packages() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = fake_binary(tmp.path());
    let archive = tmp.path().join("demo.hmmod");
    pack_demo(&archive, &bin, "demo");

    let dest = tmp.path().join("sources");
    let paths = extract_source_bundles(&archive, &dest).unwrap();
    assert_eq!(paths.len(), 1);
    assert!(paths[0].is_file());
    assert_eq!(
        sha256_file(&paths[0]).unwrap(),
        sha256_file(&fixture()).unwrap()
    );
}
