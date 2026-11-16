# hmloader — precompiled mods for Horizon

`hmloader` is a custom mod loader for [Horizon](https://github.com/horizon-rust/horizon), the
Minecraft: Java Edition 26.3 client rewritten in Rust. It introduces **`.hmmod`**: a mod package
that already contains the compiled Horizon client, so players do **not** wait 30–40 minutes for a
Rust build after installing a mod.

This repository is the mod loader, its converter and the `.hmmod` format specification. The
upstream Horizon project is forked at
[promikailamin/horizon](https://github.com/promikailamin/horizon).

**Download:** [hmloader 0.1.0 release](https://github.com/promikailamin/horizon_modloader/releases/tag/hmloader-v0.1.0)
(`hmloader-x86_64-unknown-linux-gnu`, the test `example_mod-0.1.0.hmmod`, and `SHA256SUMS`).

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

### Is this the same as the launcher's "import build"?

At runtime, yes: the payload is a compiled `horizon-client`, the same kind of artifact the official
launcher's `import-build` accepts. `hmloader import-launcher` literally hands that executable to
`horizon-launcher import-build`. What `.hmmod` adds is the packaging and automation around the
binary: a manifest with Horizon/target pins, per-file SHA-256 checks, the original `.hnmod` for
inspection, and install/verify/launch commands that work without the official launcher. A bare
`import-build` is an executable imported into one launcher installation and has no portable package
metadata of its own.

So this is not a second mod mechanism or a more powerful loader. It is the same compiled client,
packaged so it can be distributed, verified and produced automatically. The build workflow below
performs exactly that compile/import-build step for one or many mods.

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

### Build from GitHub Actions

`.github/workflows/build.yml` compiles mods on a workflow dispatch. Use the Actions UI
(**Build .hmmod** -> **Run workflow**) or:

```sh
gh workflow run build.yml \
  -f mods="tests/fixtures/example_mod-0.1.0.hnmod" \
  -f out="example_mod-0.1.0.hmmod" \
  -f profile=fast
```

`mods` accepts repository-relative paths or public URLs separated by spaces, commas or newlines.
The workflow compiles all of them together into one `.hmmod` (like a modpack), reads the Horizon pin
from the first bundle, checks out the pinned Horizon and `horizon-mixins` revisions, builds on a
Linux x86-64 runner and uploads `<out>` plus `<out>.sha256` as an artifact. Set `release_tag` to
also attach them to a GitHub release. See [docs/BUILD_WORKFLOW.md](docs/BUILD_WORKFLOW.md) for every
input and for troubleshooting.

On the 4-vCPU validation runner a warm-cache build took about 6-7 minutes total: roughly 5m42s of
Cargo compilation, about 20s of mixin application and under a minute of source export/staging.
Adding mods to the same dispatch shares that one compile; only the mixin application grows. A cold
Cargo cache can add tens of minutes.

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

## `.hnmod` vs `.hmmod`: is anything lost?

Short answer: for an ordinary mixin-only native mod, **the mod does not lose capabilities**. Both
formats end with the same `horizon-mixins apply` source transformation followed by a
`cargo build -p horizon-client`. `hmloader` does not sandbox, rewrite or interpret the mixin code;
it packages the client that build produced. There is no class of source-mixin mod that works as an
`.hnmod` but cannot be expressed as an `.hmmod`.

The differences are about builds and distribution:

| Topic | `.hnmod` (upstream) | `.hmmod` (this loader) |
|---|---|---|
| Who compiles | Every player, 30-40 min | The author, once |
| Mixin mechanism | `horizon-mixins` source mixins | Same mixin mechanism and build step |
| Platforms | Each player builds for their own machine | The author must publish one package **per target** (Linux/macOS/Windows, x86-64/arm64); `hmloader` refuses a mismatched target |
| Combining mods | Install several `.hnmod` files and rebuild together | Convert them together into one `.hmmod`; separate `.hmmod` files with different builds cannot be launched together |
| Horizon upgrade | Rebuild against the new revision | The author must reconvert; a package pins one Horizon commit |
| Build profile | Player can choose | Fixed by the author (`fast` or `release`) |
| Download size | Small source bundle | Whole prebuilt client (tens of MiB), once per target/combination |
| Trust | Built locally from source | Unsigned author-built binary; the original `.hnmod` is included under `source/` for inspection |
| Native build extras | Handled by the Horizon native-build pipeline | Not validated here: bundles with extra `[[crate]]` packages or `[[post_build]]` steps may need that same pipeline |
| Minecraft resources | Prepared separately | Same; assets are never packaged |

So an `.hmmod` cannot be a single universal file for every OS/CPU, cannot be combined at runtime
with an independently built package, and may not reproduce bundles that depend on Horizon's extra
native build steps. Those are constraints of shipping a prebuilt game, not restrictions on the mod
code itself. A runtime plugin ABI would remove most of them, but upstream has no such ABI today.

Two converter details worth knowing:

- Only `mixins/<id>/` is taken from an `.hnmod`; the rest of the bundle (for example
  `horizon-native.json`) is used as metadata and carried along as provenance, not executed.
- The `--mixins` checkout is used as given. The converter does not currently verify that it matches
  the `horizon-mixins` revision recorded in the bundle, so check it out at that revision yourself.

## Would C++ instead of Rust let a mod build alone?

No — the programming language is not what forces the whole client to be rebuilt. The **modding
model** is. Two different models are easy to confuse:

| Model | What a mod is | Does it need a full game build? |
|---|---|---|
| Compile-time source mixins (what Horizon does today) | Rust source that `horizon-mixins` patches into the game's own source | **Yes.** Any change to the game source means rebuilding `horizon-client` |
| Runtime plugin / hook ABI | A separate `.so`/`.dll` the game loads and calls through a stable interface | **No.** The mod is built and shipped independently of the game |

Horizon native mods are the first model. They rewrite Horizon's source and are then compiled into
the client, so there is nothing to build on its own. That is a property of source mixins, and it
would be exactly the same if the game were written in C++ and the mods patched C++ source: you would
still compile the whole game for every mod.

The second model is what allows one small build per mod. It requires the game to expose a stable
runtime interface — a plugin SDK, a C ABI, or a hooking/loading system — and to load mods at launch
instead of linking them in. A C++ game *can* do this, but so can Rust: a Rust `cdylib` that exports
`extern "C"` functions is a normal C-ABI plugin. Language is not the deciding factor; the presence
of that runtime boundary is.

C++ is in fact not automatically friendlier here. The C++ ABI is not stable across compilers,
compiler versions or standard-library layouts (name mangling, `std::string`/`std::vector` layout,
exceptions, RTTI), so C++ plugins still have to hide behind a stable C ABI or pin one toolchain and
compiler. Rust has the same constraint for binary plugins and solves it the same way: export a C ABI
or use a fixed toolchain. Neither language gives pluggable mods "for free".

So:

- If Horizon were a C++ game using source-level patches, this loader's `.hmmod` approach would look
  the same — still one prebuilt executable per mod or mod combination.
- If it were a C++ game designed with a stable plugin ABI, mods could be small independently built
  libraries. The equivalent is also possible in Rust today; Horizon simply does not expose such an
  ABI.
- The `.hmmod` format itself is language-agnostic: it packages a prebuilt executable plus a manifest
  of SHA-256 hashes. A C++ `horizon-client` would fit the same format unchanged.

The way to remove the per-mod full build is therefore an **upstream runtime hook/plugin ABI**, not a
change of implementation language. Until Horizon has one, `hmloader` uses the supported source-mixin
build and packages its output; building only the mod is not possible with that model in either
language.

## Limitations and future work

- A stable runtime ABI for Horizon hooks would allow many small `.hmmod` files instead of one
  prebuilt game per combination, and would remove the per-platform/per-combination build matrix.
  Upstream has no such ABI today; this loader deliberately uses the supported source-mixin build and
  packages its output.
- The source converter supports ordinary mixin bundles. Bundles using extra `[[crate]]` packages or
  `[[post_build]]` steps may need the official launcher's full native-build pipeline; those are
  candidates for a `--launcher` backend.
- Cross-compilation is not attempted: build on the platform you ship to.
- The loader does not patch the official launcher's UI; it is a separate CLI that can optionally
  import its build into the launcher.

## Licence

MIT for this loader and its original code. Horizon is MIT for its original work; dependencies,
Minecraft assets and the Minecraft EULA keep their own terms. See [LICENSE](LICENSE).
