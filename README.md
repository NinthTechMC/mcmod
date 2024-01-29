# mcmod
My CLI tool for MC mod projects.

## Concept
This tool keeps the "source files" of the mod in a small, eclipse project that can be
imported properly by jdtls, which refuses to work with any gradle project that is slightly different
from whatever the "standard" jdtls uses. It uses the `mcmod.yaml` config file to copy sources
over to a "template" project, and generate metadata and properties to make the project build.

**I don't recommend anyone using this tool, since it's personalized to my workflows**. If you do want to give it a try
please go ahead, just know that it's always unstable.

## Install
Prereq: you need these programs for either installing or running the tool:
- [Rust](https://rustup.rs/) toolchain and compiler for your platform
- Programs in `PATH`:
  - `git`
  - [`ninja`](https://ninja-build.org/) for incremental build
  - **required for windows** [`coreutils`](https://github.com/uutils/coreutils)
- Appropriate JDK version installed. See [Java Environment](#java-environment) below

1. Clone the repo
2. `cargo build --release`
3. Add `/path/to/this/repo/target/release` to `PATH`

## Java Environment
This tool uses `JDK<version>_HOME` variables to locate the JDKs.

For example, for Java 8, you should have an environment variable `JDK8_HOME` that points to, for example, `E:\jdks\jdk8u352-b08`

## Mod Build Steps
Unless otherwise specified, you should be able to follow these steps to build any mcmod project

0. Make sure you have done the stuff above
1. Clone the project and `cd` to it
2. Run `mcmod build`

## Incremental Build
If any configuration file is changed, or if files are being added/removed/renamed,
you need to run `mcmod sync` before `mcmod run`. Or run `mcmod run --sync` every time
if you are lazy (will be slower)

