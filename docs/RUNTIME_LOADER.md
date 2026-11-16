# Runtime loader: a mods folder without a Rust build

This document describes the "drop in `hmloader`, put `.hmmod` files in `mods/`, press play" mode
added for the runtime-loader request. It explains exactly what the loader can do, what it cannot do,
and why.

## Short answer

A `.hmmod` is **not** a small plugin. It is a complete prebuilt `horizon-client`. A runtime loader
can discover, verify, install and start that client, but it cannot make two independently compiled
native clients share one process. That would require a runtime plugin/hook ABI in Horizon itself,
which does not exist today.

`hmloader runtime` therefore does the closest safe thing:

1. installs every `.hmmod` in the folder;
2. groups the packages by `build_id`;
3. finds a build whose `mods` list contains every installed mod;
4. starts it; or, if no such build exists, stops and prints the `hmloader combine` command.

It never silently launches only one of several separately built mods.

## Portable loader bundle

Build a folder with the loader binary, an empty `mods/` directory and launcher scripts:

```sh
cargo build --release
./scripts/package_runtime_loader.sh \
  target/release/hmloader \
  x86_64-unknown-linux-gnu \
  loader-x86_64-unknown-linux-gnu
```

The result is:

```
loader-x86_64-unknown-linux-gnu/
  hmloader
  mods/
  run.sh
  run.cmd
  README.txt
```

On Linux/macOS run `./run.sh`; on Windows run `run.cmd`. Both run
`hmloader runtime mods`, so they work even if the current directory is elsewhere.

The **Build runtime loader** GitHub Actions workflow (`.github/workflows/loader.yml`) does this on
Linux x86-64, Windows x86-64 and macOS arm64, writes a `.tar.gz` plus SHA-256, uploads them as
artifacts and optionally attaches them to a release.

The same workflow is only about packaging `hmloader`. It does not add a new modding mechanism.

## Usage

```sh
mkdir -p loader/mods
cp my-mod-a.hmmod my-mod-b.hmmod loader/mods/
cd loader
./hmloader runtime mods --dry-run
./hmloader runtime mods
```

`--dry-run` prints the client path and command without starting the game. Arguments after `--` are
passed to Horizon:

```sh
./hmloader runtime mods --resources ~/.local/share/horizon/resources -- --width 1600 --height 900
```

Other useful commands:

```sh
./hmloader install-dir mods     # install every .hmmod in the folder
./hmloader list                 # inspect installed packages and build ids
./hmloader verify <id>          # re-check hashes and the client
```

## What "built together" means

One `.hmmod` can list several mods. That package was produced by converting several `.hnmod` files
together:

```sh
./hmloader convert mod-a.hnmod mod-b.hnmod \
  --out ab.hmmod \
  --source ./horizon \
  --mixins ./horizon-mixins
```

Its manifest looks like:

```json
{
  "id": "mod_a",
  "build_id": "same-for-the-whole-compile",
  "mods": [
    { "id": "mod_a", "version": "1.0.0" },
    { "id": "mod_b", "version": "2.0.0" }
  ]
}
```

`runtime` sees that the package provides both `mod_a` and `mod_b`, so it can start it. If the user
installed separate `a.hmmod` and `b.hmmod` builds instead, each has its own `build_id` and its own
client; neither client contains the other mod. There is no safe way to merge those executables at
runtime.

## Combining existing `.hmmod` files

Every package produced by `hmloader convert` keeps its original `.hnmod` sources under
`payload/source/`. `hmloader combine` extracts and deduplicates those sources, then invokes the normal
converter:

```sh
./hmloader combine mods \
  --out modpack.hmmod \
  --source ./horizon \
  --mixins ./horizon-mixins \
  --id my_modpack \
  --name "My Modpack" \
  --version 1.0.0
```

The default id is `modpack_<first 8 hex characters of the combined source hash>`, so repeated
combinations are deterministic and do not collide with individual packages.

This is a **build step**: it needs Rust, a Horizon source checkout containing the pinned commit, and
`horizon-mixins`. It produces one prebuilt client, just like converting the `.hnmod` files together
directly. After it succeeds:

```sh
./hmloader remove mod_a
./hmloader remove mod_b
./hmloader install modpack.hmmod
./hmloader launch modpack
```

Packages without `source/*.hnmod` (for example hand-made packages) cannot be combined; recreate them
with a current `hmloader convert`.

## Why Fabric-style runtime loading is still not possible

Fabric and Forge load small mod jars at runtime because Minecraft: Java Edition is JVM bytecode.
The JVM provides lazy class loading and bytecode transformation. Horizon is a native,
ahead-of-time-compiled Rust program. Its native mods are source mixins applied before
`cargo build -p horizon-client`; the linker has already resolved calls to concrete machine code by
launch time.

A native loader could support runtime plugins only if Horizon exported a stable plugin ABI (for
example a C ABI with an event bus or detour hooks). That boundary must be built into Horizon
upstream. `hmloader` cannot retrofit it safely from the outside, because native ABIs are
platform/toolchain specific and inlining makes blind machine-code patching unreliable.

Until then the truthful model is:

| Workflow | Result |
| --- | --- |
| One `.hmmod` built from one or more `.hnmod` files | One no-compile client containing those mods |
| Several separately built `.hmmod` files | Several clients; runtime starts one complete build, not a merge |
| `hmloader combine` | Rebuilds one client from the original sources, then runs no-compile |
| Fabric-style independent runtime plugins | Requires an upstream Horizon plugin ABI |

## Safety

`runtime` uses the same install path as `install`:

- the archive manifest is validated before extraction;
- every payload file is SHA-256 checked;
- path traversal, symlinks, duplicates and unknown files are refused;
- a target mismatch is refused;
- the prebuilt client is unsigned, so only use packages from authors you trust.

The loader does not execute scripts while installing packages.

## Tests

The repository tests cover:

- grouping packages by `build_id`;
- selecting a build that covers all installed mods;
- refusing to pretend that separately built clients can be merged;
- installing every package in a folder;
- extracting and deduplicating `.hnmod` sources from `.hmmod` archives;
- launching a complete build in dry-run mode.

Run them with `cargo test --all-targets`.
