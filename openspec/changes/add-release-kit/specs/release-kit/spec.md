## ADDED Requirements

### Requirement: Repository MUST provide an alpha macOS release kit
PrismTrace MUST provide a release kit that can package the current macOS CLI into a downloadable alpha artifact without requiring users to compile the repository from source.

#### Scenario: Maintainer packages a release artifact
- **WHEN** a maintainer runs the release packaging path
- **THEN** it produces a `.tar.gz` archive for macOS
- **AND** the archive contains a user-facing `prismtrace` executable
- **AND** the archive contains installation and checksum files

#### Scenario: Maintainer packages Apple Silicon and Intel release artifacts
- **WHEN** the release workflow runs for a release tag
- **THEN** it produces an Apple Silicon archive for `aarch64-apple-darwin`
- **AND** it produces an Intel archive for `x86_64-apple-darwin`
- **AND** each archive has its own `.sha256` checksum file

### Requirement: Installed command MUST be named prismtrace
The user-facing installed command MUST be `prismtrace`, while existing developer entrypoints such as `prismtrace-host` MAY remain available for compatibility.

#### Scenario: User runs installed binary
- **WHEN** a user installs the release kit
- **THEN** `prismtrace --discover` is the documented discovery smoke test
- **AND** the user is not required to run `cargo run` for normal usage

### Requirement: Release archive MUST include a local install script
The release archive MUST include an installation script that copies the `prismtrace` binary into a configurable prefix.

#### Scenario: User installs into a custom prefix
- **WHEN** a user runs `install.sh --prefix "$HOME/.local"`
- **THEN** the script installs the executable under `$HOME/.local/bin/prismtrace`
- **AND** it reports the installed command path

### Requirement: Release workflow MUST be separate from normal PR CI
PrismTrace MUST provide release automation that runs on release-oriented triggers without making every pull request perform release packaging.

#### Scenario: Release workflow is triggered manually
- **WHEN** a maintainer triggers the release workflow manually
- **THEN** it builds the release binary
- **AND** it uploads the packaged archive as a workflow artifact

#### Scenario: Release workflow is triggered by a version tag
- **WHEN** a maintainer pushes a `v*` tag
- **THEN** the release workflow builds the archive
- **AND** it can publish the archive and checksum to a GitHub Release

### Requirement: Documentation MUST describe install and alpha limitations
PrismTrace documentation MUST describe how users install the alpha release kit and what limitations apply to the initial distribution.

#### Scenario: User reads the README for installation
- **WHEN** a user opens the README
- **THEN** it includes Homebrew installation commands
- **AND** it includes tarball fallback installation commands
- **AND** it states that the alpha tarball release is macOS-focused and unsigned
- **AND** it keeps local development commands separate from user installation commands
