# Validation report

Environment: GitHub Actions Ubuntu runner, 4 vCPU, 15 GiB RAM, Linux x86-64, Rust 1.99.1.

## Official launcher

Downloaded the official asset `horizon-launcher-linux-x86_64.tar.gz` from release
`launcher@0.2.67`.

```
$ horizon-launcher --version
horizon-launcher 0.2.67

$ horizon-launcher install latest
Installed nightly-2026-11-13-9f42064. Prepare it next: horizon-launcher prepare --version nightly-2026-11-13-9f42064
```

`prepare` refused, as expected on this runner:

```
error: Minecraft 26.3 was not found. Horizon looked in:
  /home/runner/.minecraft (the folder doesn't exist)
```

The official launcher's native build path was also checked and correctly refused an unprepared
base:

```
error: nightly-2026-11-13-9f42064 is not prepared yet.
```

Preparing resources requires either a local Minecraft 26.3 installation or a signed-in Microsoft
account that owns Minecraft. Neither exists in CI, so the full native-build path through the
launcher could not be used. The converter therefore uses the same supported
`horizon-mixins apply` + `cargo build` pipeline directly, which needs no game assets.

## Test `.hnmod`

Source: the upstream `horizon-native-mod-template`, packaged by its own `tools/package.py`.

```
tests/fixtures/example_mod-0.1.0.hnmod
size:   1465 bytes
sha256: c41bc192214384a75fbc64b5377869da2e16dbbd2cf20e2ae6353b7a82669c0d
id:     example_mod
version 0.1.0
Horizon: ba6344a59cb9e755b730dcf02285fc0d33b97d3c
mixins:  d9c83d7096b7ea5b83678da2efdfd6161f74997e
```

The bundle adds a HEAD hook on `horizon_client::game::Game<'_>::break_block` that logs
`[example_mod] Breaking block at x, y, z`.

## Full conversion (no prebuilt binary shortcut)

```
$ hmloader convert tests/fixtures/example_mod-0.1.0.hnmod \
    --out example_mod-0.1.0-built.hmmod \
    --source horizon \
    --mixins horizon-mixins \
    --offline \
    --target-dir horizon-cargo
==> exporting Horizon ba6344a59cb9e755b730dcf02285fc0d33b97d3c
==> applying mixins
{"injections":1,"modules":1,"status":"ok",...}
==> cargo build horizon-client
Finished `fast` profile [optimized] target(s) in 5m 42s
==> wrote example_mod-0.1.0-built.hmmod (24612628 bytes, build d61e35288622, executable 66254752 bytes)
```

Artifact:

```
sha256: a628e32cc4623b9e7afce722b4b2bf686e69c47da57d77574ea54ad114fdd7c9
build:  d61e35288622cf8e5a7077c005a91412acb4acb83fa5a54cac40c3a9b6589e1c
```

A second conversion using `--prebuilt-binary` (against the same staged inputs) produced the same
SHA-256, confirming deterministic output.

The compiled client contains the mod:

```
$ strings .../horizon-client | grep -F '[example_mod]'
[example_mod] Breaking block at
```

## Install and launch

```
$ HORIZON_HMMOD_HOME=... hmloader install example_mod-0.1.0.hmmod
Installed example_mod 0.1.0 (example_mod) -> .../mods/example_mod-0.1.0.hmmod
Build d61e35288622 (1 mods)
Verified 3 files in .../store/example_mod-0.1.0-d61e35288622-a628e32cc462

$ hmloader list
ID                   VERSION      BUILD         HORIZON              MODS
example_mod          0.1.0        d61e35288622  ba6344a59cb9  example_mod 0.1.0

$ hmloader verify example_mod
OK: example_mod 0.1.0 (3 files, target x86_64-unknown-linux-gnu, Horizon ba6344a...)

$ hmloader launch example_mod -- --help
Launching example_mod 0.1.0 (example_mod d61e35288622)
  client:   .../store/.../game/horizon-client
  instance: .../instances/example_mod
Horizon — Minecraft Java Edition 26.3 in Rust

  --resources PATH        Prepared resources (horizon-prepare)
  ...
```

Exit status: 0. This proves the precompiled modded client is installed and started by the loader
without compiling anything on the player side.

An actual world was not started because the runner has no prepared Minecraft 26.3 resources and no
Microsoft account. Without resources Horizon prints, by design:

```
Horizon's prepared game files are missing: no horizon-resources.json in .../26.3
```

## Automated tests

```
$ cargo test
running 5 tests
test reads_the_test_hnmod_fixture ... ok
test rejects_path_traversal_in_archive ... ok
test detects_tampering_after_install ... ok
test packs_installs_verifies_and_launches ... ok
test cli_pack_install_list_and_dry_run ... ok

test result: ok. 5 passed; 0 failed
```

Logs are preserved under `.agent-work/logs/`:

| File | Content |
|---|---|
| `stage.log` | Upstream template staging |
| `build.log` | Upstream template full build |
| `launcher-native-build.log` | Official launcher install/build attempts |
| `convert-test.log` | `hmloader convert` with a prebuilt binary |
| `convert-full-build.log` | `hmloader convert` full compile |
| `install-test.log` | install/list/verify |
| `launch-help.log` | launching the precompiled client |


## Published release

Release: <https://github.com/promikailamin/horizon_modloader/releases/tag/hmloader-v0.1.0>

| Asset | SHA-256 |
|---|---|
| `hmloader-x86_64-unknown-linux-gnu` | `c6c466fb2a4db699688b59b339d022fd53f01a2b7d8753a80092dd293a4d8bfd` |
| `example_mod-0.1.0.hmmod` | `a628e32cc4623b9e7afce722b4b2bf686e69c47da57d77574ea54ad114fdd7c9` |

Both assets were downloaded again from the release and their hashes matched `SHA256SUMS`:

```
$ ./hmloader-dl --version
hmloader 0.1.0
$ ./hmloader-dl info example-dl.hmmod
"id": "example_mod",
"build_id": "d61e35288622cf8e5a7077c005a91412acb4acb83fa5a54cac40c3a9b6589e1c",
```
