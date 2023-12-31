# mcmod
My CLI tool for MC mod projects

## Install

Prereq: you need these programs for either installing or running the tool:
- Rust
- `git` in your `PATH`
- `ninja` in your `PATH`
- `java` - see "Java Environment" below

1. Clone the repo
2. `cargo build --release`
3. Add `/path/to/this/repo/target/release` to `PATH`

## Java Environment
This tool reads `sourceCompatibility` in `build.gradle` and supplies the
`JAVA_HOME` variable to gradle. It uses `JDK<version>_HOME` variables to locate
the JDKs.

For example, most projects in BlockPiston uses Java 8 (`sourceCompatibility = 1.8`).
You should have an environment variable `JDK8_HOME` that points to, for example, `E:\jdks\jdk8u352-b08`

## Build Artifacts
For most projects in BlockPiston, you should be able to generate the artifacts
```
mcmod build
```
