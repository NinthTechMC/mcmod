# mcmod
My CLI tool for MC mod projects

## Install
Prereq: you need these programs for either installing or running the tool:
- [Rust](https://rustup.rs/) toolchain and compiler for your platform
- Programs in `PATH`:
  - `git`
  - [`ninja`](https://ninja-build.org/)
  - **optional** [`magoo`](https://github.com/Pistonite/magoo)
  - **required for windows** [`coreutils`](https://github.com/uutils/coreutils)
- Appropriate JDK version installed. See [Java Environment](#java-environment) below

1. Clone the repo
2. `cargo build --release`
3. Add `/path/to/this/repo/target/release` to `PATH`

## Java Environment
This tool uses `JDK<version>_HOME` variables to locate the JDKs.

For example, most projects in BlockPiston uses Java 8 (`sourceCompatibility = 1.8`).
You should have an environment variable `JDK8_HOME` that points to, for example, `E:\jdks\jdk8u352-b08`

## Mod Build Steps
Unless otherwise specified, you should be able to follow these steps to build any project in BlockPiston.

0. Make sure you have done the stuff above
1. Clone the project and `cd` to it
2. Run `magoo install` (or `git submodule update --init` if you don't have magoo)
2. Run `mcmod build`

## Incremental Build
The mod projects keep the actual source separate from the forge gradle project,
mostly because jdtls refuses to work with forge on my machine for some reason.
I don't have energy to tweak jdtls to work with every mod.

If any configuration file is changed, or if files are being added/removed/renamed,
you need to run `mcmod sync` before `mcmod run`. Or run `mcmod run --sync` every time
if you are lazy (will be slower)

