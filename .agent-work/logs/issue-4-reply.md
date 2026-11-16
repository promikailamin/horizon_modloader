# Issue #4 — reply draft

Question: If this game were made with C++ instead of Rust, is a modloader-only build
possible without building the whole game for every mod?

Answer: The language is not the deciding factor; the modding model is.

- Horizon native mods are compile-time source mixins. They patch the game's own source and are then
  compiled into `horizon-client`, so there is no standalone mod artifact to build. This is identical
  in Rust or C++: source-level patches require rebuilding the client.
- Building only the mod requires a stable runtime plugin/hook boundary (a plugin SDK, a C ABI, or a
  loader that loads `.so`/`.dll` mods at launch). A C++ game can provide this, but so can Rust via a
  `cdylib` exporting `extern "C"` symbols. Horizon upstream does not expose such an ABI today.
- C++ is not automatically easier: its ABI is unstable across compilers/versions/STL layouts, so
  plugins still need a stable C ABI or a pinned toolchain, exactly like Rust binary plugins.
- The `.hmmod` container is language-agnostic: it packages a prebuilt executable plus SHA-256
  manifest hashes. A C++ client would fit the same format.

Conclusion: removing the per-mod full build needs an upstream runtime hook/plugin ABI, not a change
of implementation language. Until then `hmloader` uses the supported source-mixin build and ships
the resulting client as an `.hmmod`.

Documented in README.md, section "Would C++ instead of Rust let a mod build alone?".
