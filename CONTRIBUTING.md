# Contributing to Donut Browser

Contributions are welcome and always appreciated! 🍩

To begin working on an issue, simply leave a comment indicating that you're taking it on. There's no need to be officially assigned to the issue before you start.

## Before Starting

Do keep in mind before you start working on an issue / posting a PR:

- Search existing PRs related to that issue which might close them
- Confirm if other contributors are working on the same issue
- Check if the feature aligns with our roadmap and project goals

## Contributor License Agreement

By contributing to Donut Browser, you agree that your contributions will be licensed under the same terms as the project. You must agree to our [Contributor License Agreement](CONTRIBUTOR_LICENSE_AGREEMENT.md) before your contributions can be accepted. This agreement ensures that:

- Your contributions can be used in the open source version of Donut Browser (licensed under AGPL-3.0)
- Donut Browser can offer commercial licenses for the software, including your contributions
- You retain all rights to use your contributions for any other purpose

When you submit your first pull request, you acknowledge that you agree to the terms of the Contributor License Agreement.

## Tips & Things to Consider

- PRs with tests are highly appreciated
- Avoid adding third party libraries, whenever possible
- Unless you are helping out by updating dependencies, you should not be uploading your lock files or updating any dependencies in your PR
- If you are unsure where to start, open a discussion and we will point you to a good first issue

## Development Setup

Ensure you have the following dependencies installed:

- Node.js (see `.node-version` for exact version)
- pnpm package manager
- Latest Rust and Cargo toolchain
- [Banderole](https://github.com/zhom/banderole)
- [Tauri prerequisites guide](https://v2.tauri.app/start/prerequisites/).

## Run Locally

After having the above dependencies installed, proceed through the following steps to setup the codebase locally:

1. **Fork the project** & [clone](https://docs.github.com/en/repositories/creating-and-managing-repositories/cloning-a-repository) it locally.

2. **Create a new separate branch.**

   ```bash
   git checkout -b feature/my-feature-name
   ```

3. **Install frontend dependencies**

   ```bash
   pnpm install
   ```

4. **Build nodecar**

   Building nodecar requires you to have `banderole` installed.

   ```bash
   cd nodecar
   pnpm build
   ```

5. **Start the development server**

   ```bash
   pnpm tauri dev
   ```

This will start the app for local development with live reloading.

## Code Style & Quality

We use several tools to maintain code quality:

- **Biome** for JavaScript/TypeScript linting and formatting
- **Clippy** for Rust linting
- **rustfmt** for Rust formatting

### Before Committing

Run these commands to ensure your code meets our standards:

```bash
# Format and lint frontend code
pnpm format:js

# Format and lint Rust code
pnpm format:rust

# Run all linting
pnpm lint
```

## Building

It is crucial to test your code before submitting a pull request. Please ensure that you can make a complete production build before you submit your code for merging.

```bash
# Build the frontend
pnpm build

# Build the backend
cd src-tauri && cargo build

# Build the Tauri application
pnpm tauri build
```

Make sure the build completes successfully without errors.

## Cross-Compilation for macOS

The project supports building for both Intel (x86_64) and Apple Silicon (ARM64) architectures on macOS.

### Architecture-Specific Builds

The `nodecar` sidecar binary can be built for specific architectures using platform-specific build scripts:

```bash
cd nodecar

# For Apple Silicon (ARM64)
pnpm run build:mac-aarch64

# For Intel (x86_64)
pnpm run build:mac-x86_64
```

These scripts use the `--arch` flag with `banderole` to specify the target architecture:
- `--arch arm64` for Apple Silicon M-series chips
- `--arch x64` for Intel processors

### How It Works

The build process uses [banderole](https://github.com/zhom/banderole), a Node.js bundler that creates portable single-executable binaries. When the `--arch` flag is specified:

1. Banderole downloads the correct Node.js binary for the target architecture
2. It bundles your TypeScript code (compiled to JavaScript) with the Node.js runtime
3. The resulting binary can run natively on the target architecture

This allows cross-compilation on macOS, meaning you can build x86_64 binaries on an ARM64 Mac and vice versa, which is essential for creating universal releases that work on both architectures.

### Why This Matters

Previously, the build scripts didn't specify the target architecture, causing banderole to default to the host machine's architecture. This meant:
- Building on ARM64 (M-series) Macs would always produce ARM64 binaries, even for the `build:mac-x86_64` target
- Intel Mac users couldn't run the application built on ARM64 machines

With explicit `--arch` flags, the build system correctly produces binaries for both architectures from a single build machine.

## Testing

- Always test your changes on the target platform
- Verify that existing functionality still works
- Add tests for new features when possible

## Pull Request Guidelines

🎉 Now that you're ready to submit your code for merging, there are some points to keep in mind:

### PR Description

- Fill your PR description template accordingly
- Have an appropriate title and description
- Include relevant screenshots for UI changes. If you can include video/gifs, it is even better.
- Reference related issues

### Linking Issues

If your PR fixes an issue, add this line **in the body** of the Pull Request description:

```text
Fixes #00000
```

If your PR is referencing an issue:

```text
Refs #00000
```

### PR Checklist

- [ ] Code follows our style guidelines
- [ ] I have performed a self-review of my code
- [ ] I have commented my code, particularly in hard-to-understand areas
- [ ] I have made corresponding changes to the documentation
- [ ] My changes generate no new warnings
- [ ] I have added tests that prove my fix is effective or that my feature works
- [ ] New and existing unit tests pass locally with my changes
- [ ] Any dependent changes have been merged and published

### Options

- Ensure that "Allow edits from maintainers" option is checked

## Architecture Overview

Donut Browser is built with:

- **Frontend**: Next.js React application
- **Backend**: Tauri (Rust) for native functionality
- **Node.js Sidecar**: `nodecar` binary for access to JavaScript ecosystem
- **Build System**: GitHub Actions for CI/CD

Understanding this architecture will help you contribute more effectively.

## Getting Help

- **Issues**: Use for bug reports and feature requests
- **Discussions**: Use for questions and general discussion
- **Pull Requests**: Use for code contributions

## Code of Conduct

Please note that this project is released with a [Contributor Code of Conduct](CODE_OF_CONDUCT.md). By participating in this project you agree to abide by its terms.

## Recognition

All contributors will be recognized! We use the all-contributors specification to acknowledge everyone who contributes to the project.

---

Thank you for contributing to Donut Browser! 🍩✨
