# Building `.hmmod` files with GitHub Actions

[`.github/workflows/build.yml`](../.github/workflows/build.yml) turns one or more `.hnmod` source
bundles into a single precompiled `.hmmod`. It is the CI version of `hmloader convert`, so the
result is exactly the same kind of compiled `horizon-client` that the official launcher accepts with
`import-build`, wrapped in the verified `.hmmod` package.

## Run it

From the Actions UI, open **Build .hmmod** and choose **Run workflow**. Or use the CLI:

```sh
gh workflow run build.yml \
  --repo promikailamin/horizon_modloader \
  -f mods="tests/fixtures/example_mod-0.1.0.hnmod" \
  -f out="example_mod-0.1.0.hmmod" \
  -f profile=fast
```

Watch it and download the artifact:

```sh
gh run list --workflow build.yml
gh run watch
gh run download --name hmmod-example_mod-0.1.0.hmmod-<run-id>
```

Each run uploads the `.hmmod`, its `.sha256` file and the conversion log. The run summary shows the
mods, pins, profile, build time, size and SHA-256.

## Inputs

| Input | Required | Default | Meaning |
|---|---|---|---|
| `mods` | yes | the test fixture | `.hnmod` files separated by spaces, commas or newlines. Repository paths and public `http(s)` URLs are accepted. |
| `out` | yes | `example_mod-0.1.0.hmmod` | Output file name. Must end in `.hmmod`; it may not contain a path. |
| `profile` | yes | `fast` | Cargo profile: `fast` or `release`. |
| `horizon_repo` | yes | `https://github.com/horizon-rust/horizon` | Horizon Git repository to stage. Use a fork if the bundle pins a fork commit. |
| `mixins_repo` | yes | `https://github.com/horizon-rust/horizon-mixins` | `horizon-mixins` Git repository. |
| `mixins_revision` | no | pin from the first `.hnmod` | Override the `horizon-mixins` commit. |
| `horizon_version` | no | game version from the first `.hnmod` | Human-readable base recorded in the manifest. |
| `allow_mismatch` | no | `false` | Allow bundles that pin different Horizon or `horizon-mixins` revisions. The first pin is used. Expert use only. |
| `release_tag` | no | empty | When set, upload the `.hmmod` and `.sha256` to this GitHub release, creating it if needed. |

The Horizon commit is always taken from the first bundle's `horizon-native.json`. The workflow does
not let you silently build against a different Horizon revision, because the resulting mixins would
not match the bundle's pin.

Multiple bundles are compiled together into one client, like a modpack. They must have distinct mod
ids and, by default, the same Horizon and `horizon-mixins` pins. Add all mods to the same `mods`
input instead of dispatching separate builds when you want them to run together.

## What the workflow does

1. Builds `hmloader` from this repository with `cargo build --release --locked`.
2. Runs [`scripts/resolve_build_inputs.py`](../scripts/resolve_build_inputs.py), which copies or
   downloads every `.hnmod`, reads its `horizon-native.json`, rejects duplicate ids and incompatible
   pins, and records the resolved paths and revisions.
3. Checks out Horizon at the pinned commit and `horizon-mixins` at its pinned (or overridden)
   commit.
4. Restores a Cargo target cache keyed by the Horizon revision, `horizon-mixins` revision and
   profile.
5. Runs `hmloader convert` with all bundles, then writes `SHA256SUMS`-style `<out>.sha256` and the
   run summary.
6. Uploads the artifact and, if `release_tag` is set, uploads it to that release.

## Time

The validation build in [VALIDATION.md](VALIDATION.md) compiled the test bundle on a 4-vCPU Ubuntu
runner with a warm Cargo cache:

| Phase | Time |
|---|---|
| `horizon-mixins apply` | about 20s |
| `cargo build -p horizon-client` (`fast`) | about 5m42s |
| Source export, staging and packaging | under a minute |
| **Total** | **about 6-7 minutes** |

The Cargo compile dominates and depends on the runner and cache:

- **One mod:** one mixin pass plus one Horizon compile.
- **Several mods in the same dispatch:** still one Horizon compile. Every bundle adds a mixin pass,
  which is seconds to tens of seconds, not another full build.
- **Several separate dispatches:** each dispatch pays its own full compile. They cannot be launched
  together either, because each `.hmmod` contains its own client.
- **Cold Cargo cache:** expect tens of minutes, especially on a 2-core runner.
- **`release` profile:** slower than `fast` because of thin LTO.

The workflow caches `.agent-work/build/horizon-cargo` to make repeat builds faster. The cache is
keyed by the Horizon revision, `horizon-mixins` revision, profile and runner OS.

## Relationship to the launcher's import build

The final payload is the same compiled client the official launcher imports. The difference is the
container and the verifiable build inputs:

- `.hmmod` is portable: manifest, Horizon/target pins, per-file SHA-256, the original `.hnmod` under
  `source/`, and `hmloader install/verify/launch` without the official launcher.
- `horizon-launcher import-build` imports a bare executable into one launcher installation. It is
  not a distributable package with its own manifest and checksums.
- `hmloader import-launcher` bridges both: it verifies an installed `.hmmod` and passes its client
  to `horizon-launcher import-build --trust-unsigned`.

## Limitations

- The hosted runner is Linux x86-64, so the artifact targets `x86_64-unknown-linux-gnu`. Build on a
  matching machine for macOS, Windows or arm64; `hmloader` refuses a target mismatch.
- A release upload needs the workflow token to have `contents: write`, which the workflow declares.
- Private `.hnmod` URLs are not authenticated. Put private bundles in the repository, in a public
  release, or download them in an earlier step and pass a path.
- The workflow does not prepare Minecraft resources. The resulting `.hmmod` still uses the resources
  prepared by the official launcher.
- The prebuilt client is unsigned. Treat third-party `.hnmod` inputs as code you are choosing to
  compile and distribute.

## Troubleshooting

| Symptom | Cause |
|---|---|
| `Shallow fetch failed; fetching the full history` | The pinned commit is not reachable from the default shallow fetch; the workflow retries with a full fetch. |
| `bundles pin different Horizon revisions` | Combine only compatible bundles, build them separately, or set `allow_mismatch` after checking the source. |
| `built horizon-mixins but cannot find its binary` | The `horizon-mixins` checkout does not have the expected `horizon-mixins-cli` package. Use the repository/revision that matches the bundle. |
| `cargo finished but ... was not produced` | The requested profile name is not available or the build failed earlier; check the uploaded build log. |
| Artifact upload fails with "no files found" | The conversion step failed before writing the output; fix that step first. |
