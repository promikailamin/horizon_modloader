# Issue #7 — runtime loader reply

Question: Can you make a build that I import to my game and that loads all `.hmmod` files at runtime
like Fabric Loader?

Implemented in commit `466b983` (pushed to `main`).

## The honest limitation

`.hmmod` is not a small runtime plugin. It is a complete prebuilt `horizon-client`. Fabric can load
small jars at runtime because the JVM transforms and loads bytecode; Horizon native mods are Rust
source mixins compiled into the client. Two independently compiled native clients cannot be spliced
into one process from outside. That needs an upstream Horizon runtime plugin/hook ABI, which does
not exist today.

The new runtime mode does the closest safe workflow instead of silently launching only one mod.

## Added

- `hmloader runtime [dir]` — drop-in mode. Installs every `.hmmod` in the folder, groups packages by
  `build_id`, finds a build whose `mods` list contains every installed mod, and launches it. If the
  packages were built separately, it stops and prints the exact `combine` command instead of
  pretending they can be merged.
- `hmloader install-dir [dir]` — installs a whole folder of `.hmmod` files.
- `hmloader combine <dir> --out ... --source ... --mixins ...` — extracts the original `.hnmod`
  sources that every `.hmmod` carries under `source/`, deduplicates them and rebuilds one combined
  prebuilt `modpack_<hash>.hmmod`. This is a build step, not runtime plugin loading.
- `--id`, `--name`, `--version` overrides for `convert` and `combine`, so a modpack has its own
  identity.
- Portable loader bundle: `scripts/package_runtime_loader.sh` creates a folder with `hmloader`,
  `mods/`, `run.sh`, `run.cmd` and a README.
- `.github/workflows/loader.yml` — **Build runtime loader** dispatch that packages Linux x86-64,
  Windows x86-64 and macOS arm64 bundles with SHA-256, uploads artifacts and can attach them to a
  release.
- `docs/RUNTIME_LOADER.md`, README section, release notes and tests.
- Safer install: refuses to overwrite an existing `.hmmod` for the same id/version with a different
  build.

## Usage

```sh
mkdir -p loader/mods
cp *.hmmod loader/mods/
cd loader
./hmloader runtime mods --dry-run
./hmloader runtime mods
```

To merge separately built packages:

```sh
./hmloader combine mods \
  --out modpack.hmmod \
  --source ./horizon \
  --mixins ./horizon-mixins
./hmloader remove mod_a
./hmloader remove mod_b
./hmloader install modpack.hmmod
./hmloader launch modpack
```

## Validation actually run

- `cargo fmt --all --check` — passed.
- `cargo clippy --all-targets -- -D warnings` — passed.
- `cargo test --all-targets` — passed: 7 unit + 5 CLI + 3 runtime tests = 15 tests.
- `actionlint .github/workflows/*.yml` — passed.
- Ran `scripts/package_runtime_loader.sh` on the built loader and executed the generated `run.sh
  --help` successfully.

Not run in this session: a full `combine` Horizon compile (needs the Horizon/mixins checkouts and a
long Cargo build) and the new GitHub Actions workflow (it is a `workflow_dispatch`; syntax checked
with actionlint only). Interactive gameplay still needs licensed Minecraft resources.

Logs: `.agent-work/logs/issue-7-tests.log`, `issue-7-actionlint.log`,
`issue-7-loader-bundle.log`.
