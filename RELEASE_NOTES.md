# hmloader 0.1.0

First release of the precompiled `.hmmod` mod loader for Horizon.

## Added

- `.hmmod` format 1: deterministic ZIP with a JSON manifest, per-file SHA-256, target and Horizon
  revision pins, and the original `.hnmod` source for transparency.
- `hmloader` CLI: `info`, `install`, `list`, `remove`, `verify`, `launch`, `convert`, `pack`,
  `import-launcher`, `doctor`.
- `.hnmod` -> `.hmmod` converter that stages the pinned Horizon source with `horizon-mixins`,
  compiles `horizon-client` once on the author machine and packages the result.
- Combining several `.hnmod` files into one prebuilt `.hmmod`.
- Optional import of a prebuilt client into the official Horizon launcher (tested with launcher
  0.2.67: the build was imported and selected as its own version).
- Path-traversal, symlink, duplicate and checksum validation; no scripts run during install.
- Tests for packaging, installation, tampering detection, traversal rejection, `.hnmod` reading,
  launch and the CLI.
- Full user, author and format documentation.

## Test package

`example_mod-0.1.0.hmmod` is the included test `.hnmod` compiled against Horizon
`ba6344a59cb9e755b730dcf02285fc0d33b97d3c` with profile `fast`.

```
sha256 a628e32cc4623b9e7afce722b4b2bf686e69c47da57d77574ea54ad114fdd7c9
```

It is a Linux x86-64 build and needs prepared Minecraft 26.3 resources to play.

## Known limitations

- A native Horizon mod is a compile-time source mixin, so an `.hmmod` contains a whole prebuilt
  client. Several mods must be converted together; arbitrary small runtime plugins need an upstream
  hook ABI that does not exist yet.
- Builds are platform-specific and pinned to one Horizon commit.
- The CI runner cannot prepare Minecraft 26.3 resources without a licensed account, so interactive
  gameplay was not run here. Install, verification, launch and the client's `--help` path were run.
