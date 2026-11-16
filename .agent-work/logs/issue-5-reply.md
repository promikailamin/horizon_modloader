# Issue #5 — reply draft

Question: Vanilla Minecraft is made with Java and people made Fabric Loader, Forge Loader, etc. How
do they work, and doesn't that approach work for Horizon?

Answer: Fabric/Forge exploit JVM runtime class loading; Horizon is a native AOT binary with
compile-time source mixins, so the same loader model does not apply until Horizon adds a runtime
plugin/hook ABI.

How the Java loaders work:

- Minecraft: Java Edition ships as JVM bytecode in `.jar` files.
- Fabric Loader discovers separate mod jars, reads `fabric.mod.json`, registers entry points/events
  and applies Mixin bytecode transformations to vanilla classes as they are loaded. Fabric API is a
  jar of shared hooks. Minecraft is never recompiled.
- Forge uses the same runtime idea with a heavier stack: `ModLauncher`, `mods.toml`, an event bus,
  access transformers, coremods and Mixin-based transformations. It also patches/loads at runtime
  instead of rebuilding the game.
- Both depend on JVM features: lazy `ClassLoader`s, portable bytecode that can be rewritten before a
  class is defined, reflection, and a discoverable mod API. The mod is a small independent artifact;
  the loader splices it into the running program.

Why that does not carry over to Horizon:

- Horizon is a native, ahead-of-time compiled Rust executable. There is no JVM, no class loader, no
  portable bytecode to transform at launch and no reflective plugin scan.
- Horizon's supported native mods (`.hnmod`) are compile-time source mixins: `horizon-mixins`
  rewrites Horizon's Rust source, then `cargo build -p horizon-client` links the mod into a new
  executable. There is no standalone mod artifact for a runtime loader to load.
- `hmloader`'s `.hmmod` is the resulting prebuilt modded client packaged with a SHA-256 manifest. It
  is the native equivalent of "ship the already-modded game", not of a Fabric/Forge jar.

Could Horizon get a Fabric-like loader? Yes, in principle, by adding an upstream runtime plugin
system: load `.so`/`.dylib`/`.dll` at startup, expose a stable C ABI with hooks/events, and
optionally patch machine code with detours (the native analogue of a bytecode transformer). That
boundary must be built into Horizon itself; it does not exist today. Retrofitting it from outside is
much harder than on the JVM because the code is already compiled, native ABIs are
platform/toolchain-specific and inlining makes blind patching unreliable.

Conclusion: the difference is runtime bytecode transformation with a plugin loader (Fabric/Forge) vs
compile-time source mixins linked into a native binary (Horizon today), not Java vs Rust. If Horizon
ships a stable runtime plugin/hook ABI, a loader could load small independent mod binaries like
Fabric; until then `.hmmod` ships the compiled client.

Documented in README.md, section "How Fabric and Forge work, and why that does not carry over to
Horizon".
