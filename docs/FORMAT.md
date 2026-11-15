# `.hmmod` format 1

An `.hmmod` is a ZIP archive. It is deterministic: entries are written in sorted path order with a
fixed timestamp (`1980-01-01`), so identical inputs give identical bytes.

## Layout

```
hmmod.json                       manifest (not hashed; it holds the hashes)
payload/game/horizon-client      the prebuilt client (executable)
payload/game/native-mods.json    build metadata read by tools and crash reports
payload/source/<name>.hnmod      original source bundle(s), for transparency
```

The `payload/` prefix is stripped when the archive is unpacked into the store, so the installed
directory is directly the payload root.

## Manifest (`hmmod.json`)

```json
{
  "format": 1,
  "kind": "prebuilt-game",
  "id": "example_mod",
  "name": "Example Mod",
  "version": "0.1.0",
  "horizon_revision": "ba6344a59cb9e755b730dcf02285fc0d33b97d3c",
  "horizon_version": "26.3",
  "target": "x86_64-unknown-linux-gnu",
  "profile": "fast",
  "build_id": "d61e35288622cf8e5a7077c005a91412acb4acb83fa5a54cac40c3a9b6589e1c",
  "executable": "game/horizon-client",
  "source_sha256": "913b7b153708d79d9aa6e66667fe67b0f02b8a0cceea00a9a1882782b8c3425d",
  "source_bundle": "example_mod-0.1.0.hnmod",
  "mods": [
    { "id": "example_mod", "version": "0.1.0" }
  ],
  "files": {
    "game/horizon-client": "1efd…",
    "game/native-mods.json": "250d…",
    "source/example_mod-0.1.0.hnmod": "c41b…"
  },
  "notes": "Precompiled Horizon client for 1 mod(s). …"
}
```

| Field | Rule |
|---|---|
| `format` | Must be `1`. |
| `kind` | Format 1 defines only `prebuilt-game`. |
| `id` | `[a-z0-9_]{1,64}`, same rule as Horizon mod ids. |
| `name`, `version` | Non-empty display strings. |
| `horizon_revision` | Hex commit id of the Horizon source the client was built from. |
| `horizon_version` | Optional human-readable base (release tag, Nightly or game version). |
| `target` | Rust target triple the executable runs on. `hmloader` refuses a mismatch. |
| `profile` | Cargo profile (`fast`, `release`, `dev`). |
| `build_id` | SHA-256 over the format, revision, input hashes, profile and target. Mods with the same id can share one build. |
| `executable` | Relative path under `payload/`; must be listed in `files`. |
| `source_sha256` | SHA-256 over the ordered input `.hnmod` hashes (combined provenance). |
| `source_bundle` | Display name of the primary `.hnmod`. |
| `mods` | Every mod compiled into the build. |
| `files` | Every payload file (`payload/`-relative) -> lowercase hex SHA-256. |
| `notes` | Optional free text. |

## Validation performed by `hmloader`

Before extracting anything, it:

1. reads and validates `hmmod.json`;
2. requires `format = 1`, `kind = "prebuilt-game"`, a valid id, a hex Horizon revision, and a
   non-empty target/profile/build id;
3. requires the executable to be declared and listed;
4. rejects absolute paths, `..`, backslashes and drive-letter-like names.

While extracting, it:

1. refuses entries outside `payload/`;
2. refuses duplicate names;
3. refuses files not listed in `files`;
4. hashes every extracted file and compares it with the manifest;
5. requires every manifest entry to be present;
6. gives only the declared executable `0755`; other files get `0644`;
7. extracts into a temporary directory and renames it only after complete success.

Unknown `.hmmod` format versions, kinds, targets and mod ids are rejected rather than guessed.

## Build determinism

`hmloader convert` uses:

```
build_id = SHA-256(
  "hmmod-format-1\0" +
  horizon_revision + "\0" +
  combined_source_sha + "\0" +
  profile + "\0" +
  target
)
```

The source tree comes from `git archive <revision>`, not the current checkout, so local changes
cannot leak into a package. The work directory for a build id is reused only when its
`.horizon-revision` marker matches.

## Multiple mods

`mods` may list several entries when several `.hnmod` files were converted together. The archive
still contains one executable. `hmloader launch` allows multiple installed `.hmmod` files only when
their `build_id` is identical; otherwise it asks you to choose one or build them together.
