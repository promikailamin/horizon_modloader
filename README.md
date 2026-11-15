# hmloader — precompiled mods for Horizon

`hmloader` is a custom mod loader for [Horizon](https://github.com/horizon-rust/horizon), the
Minecraft: Java Edition 26.3 client rewritten in Rust. It introduces **`.hmmod`**: a mod package
that already contains the compiled Horizon client, so players do **not** wait 30–40 minutes for a
Rust build after installing a mod.

This repository is the mod loader, its converter and the `.hmmod` format specification. The
upstream Horizon project is forked at
[promikailamin/horizon](https://github.com/promikailamin/horizon).

> This project is not affiliated with Mojang, Microsoft or the Horizon Team. Horizon, Minecraft
> and their assets keep their own licences. Prepared Minecraft resources are private and are never
> included in an `.hmmod`.

## The problem

Horizon has two mod kinds:

| Upstream kind | Extension | What it contains | When it is built |
|---|---|---|---|
| Content mod | `.hmod` | Data and assets only | Never compiled |
| Native mod | `.hnmod` | Rust source for `horizon-mixins` | Compiled into a complete `horizon-client` for every player |

A native mod is not a plugin loaded at runtime. It rewrites Horizon's own Rust source and then
compiles the whole client. That is why an `.hnmod` takes 30–40 minutes to install on a slow machine,
and why installing a second mod rebuilds the game again.

`hmloader` moves that build to the mod author:

1. The author converts an `.hnmod` once with `hmloader convert`.
2. The converter stages the pinned Horizon source, applies the mixins, builds `horizon-client`, and
   packs the binary into an `.hmmod`.
3. Players install the `.hmmod` and launch it. No Rust toolchain, no Cargo, no compile.

## How it works

```
.hnmod (Rust source)
        │  hmloader convert  (author side, once)
        ▼
.hmmod (ZIP: manifest + prebuilt horizon-client + provenance)
        │  hmloader install  (player side, seconds)
        ▼
store/<id>-<version>-<build>/game/horizon-client
        │  hmloader launch
        ▼
Horizon starts with the mod already compiled in
```

An `.hmmod` has `hmmod.json` at its root and a `payload/` tree. Every payload file is SHA-256
listed in the manifest and checked on install, verify and launch. Paths are validated, symlinks
are refused, and the prebuilt executable is the only file that gets execute permission. The
complete binary format is in [docs/FORMAT.md](docs/FORMAT.md).

### Why an `.hmmod` carries a whole client

Because Horizon native mods are compile-time source mixins, a precompiled mod cannot be a small
`.so`/`.dll` plugged into an unmodified game: there is no stable runtime plugin ABI for arbitrary
mixins. To guarantee that a player never compiles anything, the `.hmmod` carries the game binary
that was produced with the mod.

Consequences, stated plainly:

- One `.hmmod` is one prebuilt game. It runs alone.
- To combine several mods, convert them **together** into one `.hmmod`. The result is a modpack-like
  single build and uses exactly the same no-compile install.
- Builds are platform-specific (`x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`, …) and pin one
  Horizon commit. `hmloader` refuses a package built for another target.
- The `.hmmod` does not contain Minecraft resources. Horizon still needs the Minecraft 26.3
  resources prepared by the official launcher (or `horizon-prepare`). Those files stay private and
  outside the package.

This is the same trade-off as shipping a prebuilt modded game: you trade disk space and one build per
combination for a zero-compile player experience. A runtime hook ABI would be a much larger change
to Horizon itself; it is described as future work below.

## Requirements

### Player

- Linux x86-64/aarch64, macOS x86-64/arm64, or Windows x86-64/arm64 that can run Horizon itself.
- The official **Horizon launcher** (recommended) or prepared Minecraft 26.3 resources for Horizon.
- A downloaded `.hmmod` file.
- No Rust, no Cargo, no JDK for installing and launching an `.hmmod`.

### Mod author / converter

- The same platform you build for.
- Rust 1.95 or newer (`rustc` and `cargo` on `PATH`).
- `git`, `tar`, Bash (Linux/macOS; Git Bash or equivalent on Windows).
- A Horizon source checkout containing the commit pinned by the `.hnmod`.
- A `horizon-mixins` checkout at the revision pinned by the `.hnmod`.
- Cargo registry access once (or a warm Cargo cache with `--offline`).

## Quick start (player)

Download `hmloader` from the latest release, then:

```sh
chmod +x hmloader
./hmloader --version
```

Install a package that is already in your downloads folder:

```sh
./hmloader install ~/Downloads/example_mod-0.1.0.hmmod
```

If you prefer the "put it in the mods folder" workflow, find the mods folder:

```sh
./hmloader doctor
# mods: /home/you/.local/share/horizon-hmmod/mods
```

Copy any `.hmmod` there, then run:

```sh
./hmloader list
./hmloader install ~/.local/share/horizon-hmmod/mods/example_mod-0.1.0.hmmod
./hmloader verify example_mod
```

`install` copies the archive into the mods folder and unpacks the verified client into the store.
Re-running it for the same build is a no-op.

Launch the game:

```sh
./hmloader launch
# or choose explicitly:
./hmloader launch example_mod
```

Anything after `--` goes to Horizon:

```sh
./hmloader launch example_mod -- --width 1600 --height 900 --creative
```

### Resources

Horizon needs Minecraft 26.3 resources. `hmloader launch` uses, in order:

1. `--resources PATH` if you pass it;
2. `HORIZON_RESOURCES`;
3. `$HORIZON_HOME/resources`;
4. `~/.local/share/horizon/resources`;
5. `<instance>/resources`, where the instance is
   `~/.local/share/horizon-hmmod/instances/<mod-id>/`.

The simplest path is to install and prepare Horizon 26.3 with the official launcher once, then let
`hmloader` find the prepared folder, or pass it explicitly:

```sh
./hmloader launch example_mod --resources ~/.local/share/horizon/resources
```

Saves, options and logs of the modded instance live in
`~/.local/share/horizon-hmmod/instances/<mod-id>/`, so different `.hmmod` builds do not overwrite
each other.

### Use with the official launcher UI

If you want the official launcher to supply its prepared resources and account integration, import
the prebuilt client as its own launcher version:

```sh
./hmloader import-launcher example_mod \
  --launcher ~/.local/bin/horizon-launcher \
  --version nightly-2026-11-13-9f42064
```

This runs `horizon-launcher import-build <prebuilt-client> --trust-unsigned`, which is exactly the
official launcher's own "install a game program I have" path. You can then select that version in
the launcher. Importing is optional; `hmloader launch` works without it.

## Making an `.hmmod` (mod author)

### 1. Get the source revisions

`hmloader convert` needs a Horizon Git checkout that contains the commit pinned in the `.hnmod`,
and a `horizon-mixins` checkout. The revisions are in the bundle's `horizon-native.json`.

Example with the test bundle in this repository (`ba6344a…`, Nightly 2026-10-30):

```sh
git clone --filter=blob:none https://github.com/horizon-rust/horizon horizon
git -C horizon fetch --depth 1 origin ba6344a59cb9e755b730dcf02285fc0d33b97d3c

git clone https://github.com/horizon-rust/horizon-mixins horizon-mixins
git -C horizon-mixins checkout d9c83d7096b7ea5b83678da2efdfd6161f74997e
```

The pinned commit must exist in the checkout; a shallow clone may need the `fetch` above.

### 2. Convert

```sh
./hmloader convert tests/fixtures/example_mod-0.1.0.hnmod \
  --out example_mod-0.1.0.hmmod \
  --source ./horizon \
  --mixins ./horizon-mixins
```

The first run downloads/stages the Horizon source, applies the mixins, fetches the locked Cargo
dependencies and builds `horizon-client`. On a 4-core machine the test bundle took about 6 minutes
with a warm Cargo cache; a first build with an empty cache takes longer. The result is deterministic:
the same source, bundle, profile and target produce the same archive bytes.

Useful options:

| Option | Meaning |
|---|---|
| `--profile fast` | Horizon's incremental performance profile (default) |
| `--profile release` | Thin-LTO release build (slower build, slightly faster game) |
| `--target-dir PATH` | Reuse Cargo artifacts between conversions |
| `--work-dir PATH` | Where staged source and mods live (default: `<home>/work`) |
| `--offline` | Never let Cargo access the network; use the existing cache |
| `--mixins-cli PATH` | Use an already-built `horizon-mixins` binary |
| `--horizon-version NAME` | Human-readable base name recorded in the manifest |
| `--prebuilt-binary PATH` | Skip compilation and package an existing client |

### 3. Combine several `.hnmod` mods into one package

```sh
./hmloader convert mod-a.hnmod mod-b.hnmod mod-c.hnmod \
  --out pack-1.0.0.hmmod \
  --source ./horizon \
  --mixins ./horizon-mixins
```

All bundles must pin the same Horizon revision by default. The converter refuses duplicates and
mismatched pins; `--allow-mismatch` is deliberately an expert escape hatch and does not guarantee
that the mixins apply together.

### 4. Publish

Attach the `.hmmod` to a GitHub release and publish its SHA-256 from `sha256sum`. Players only need
the file and `hmloader`; they never need your source.

## Command reference

```text
hmloader info <archive>            inspect an .hmmod without installing it
hmloader install <archive>         verify, unpack and add it to mods/
hmloader list                      list installed mods and build ids
hmloader verify <id>               re-check every hash and the client
hmloader remove <id>               remove an installed mod and its files
hmloader launch [id] [OPTIONS]     run the prebuilt client
hmloader convert <hnmod...> ...    build one or more .hnmod files into an .hmmod
hmloader pack ...                  package an existing client (advanced)
hmloader import-launcher <id> ...  import a build into the official launcher
hmloader doctor                    environment and installation diagnostics
```

Global option: `--home PATH` or `HORIZON_HMMOD_HOME` changes the data folder. The default is
`$XDG_DATA_HOME/horizon-hmmod`, else `~/.local/share/horizon-hmmod`.

## Safety model

- Native Horizon mods run unsandboxed with your user's rights. Only install `.hmmod` files from
  authors you trust. Packaging a build does not make its code safer.
- `hmloader` verifies the SHA-256 of every payload file and refuses unknown files, unsafe paths and
  symlinks. It does not execute scripts during install.
- The `.hmmod` includes the original `.hnmod` under `source/` for transparency, and its hash in the
  manifest. You can inspect the Rust source before running a package.
- The prebuilt client is unsigned. The official launcher calls this "trust unsigned"; `import-launcher`
  passes that switch only because you explicitly asked to import the package.
- Never put Mojang/Microsoft assets, prepared resources, tokens or account files in an `.hmmod`.

## Building `hmloader`

```sh
cargo build --release
./target/release/hmloader --help
```

Tests:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

The test suite covers the ZIP format, path-traversal rejection, checksum tampering, `.hnmod` reading,
installation, launch dry-runs and the CLI. The real conversion of the included
`tests/fixtures/example_mod-0.1.0.hnmod` is documented in [docs/VALIDATION.md](docs/VALIDATION.md).

## Tested in this repository

- Downloaded the official Horizon launcher `0.2.67` and installed Nightly
  `nightly-2026-11-13-9f42064`.
- Built the included test `.hnmod` with Horizon commit `ba6344a…` and horizon-mixins `d9c83d7…`.
- Converted it end-to-end with `hmloader convert` (full compile, no `--prebuilt-binary`).
- Installed, listed, verified and launched the resulting `.hmmod`; `hmloader launch example_mod --
  --help` printed the Horizon 26.3 command line from the prebuilt modded client.
- Confirmed the compiled binary contains the mod's `[example_mod] Breaking block at` string.
- Confirmed byte-for-byte deterministic `.hmmod` output between a build using an existing binary
  and a full compile.

Full logs and exact hashes are in [docs/VALIDATION.md](docs/VALIDATION.md) and `.agent-work/logs/`.

Not tested here: interactive gameplay, because this CI environment has no Microsoft account that
owns Minecraft and no legally prepared Minecraft 26.3 resources. That is an asset/account boundary,
not a code path; the client starts and parses its command line from the prebuilt package.

## Limitations and future work

- A stable runtime ABI for Horizon hooks would allow many small `.hmmod` files instead of one
  prebuilt game per combination. Upstream has no such ABI today; this loader deliberately uses the
  supported source-mixin build and packages its output.
- The source converter supports ordinary mixin bundles. Bundles using extra `[[crate]]` packages or
  `[[post_build]]` steps may need the official launcher's full native-build pipeline; those are
  candidates for a `--launcher` backend.
- Cross-compilation is not attempted: build on the platform you ship to.
- The loader does not patch the official launcher's UI; it is a separate CLI that can optionally
  import its build into the launcher.

## Licence

MIT for this loader and its original code. Horizon is MIT for its original work; dependencies,
Minecraft assets and the Minecraft EULA keep their own terms. See [LICENSE](LICENSE).
