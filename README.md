# LaunchShelf

**A home for the apps you build.**

LaunchShelf is a native Rust desktop workshop for local Linux projects. Add a
source folder, review the suggested settings, and launch or build your app with
its documentation and process log close at hand.

Version 0.1 is an early development version, tested locally on Omarchy. Broader
Linux and clean-system verification remain pending. Build from source using the
instructions below; a packaged binary release is not available yet.

## Features

- Native egui interface with project cards, search, favorites and type filters.
- Reviewable detection for Cargo, npm/pnpm/yarn, Python entry scripts and `play` scripts.
- Import installed desktop apps through their `.desktop` entries.
- Configure launch/build commands, working directory, documentation and terminal mode.
- Read Git branch and working-tree change count without changing repositories.
- Retain launch/build logs, including missing-command and nonzero-exit failures.
- Register managed desktop launchers; remove library entries without deleting source.
- Keep the private library outside the source repository, with atomic saves,
  a previous-file backup and protection against stale concurrent-window saves.

Detection creates a proposal; it does not install dependencies or run commands.
Python detection currently recognizes `app.py` and `main.py`; generic desktop
entry import delegates execution to `gtk-launch`, respecting the desktop system's
handling of file arguments and terminal settings.

## Build and run

Requires Linux, Rust 1.95 or newer, a Wayland or X11 desktop, and OpenGL/EGL.
Development packages vary by distribution. Ubuntu/Debian:

```sh
sudo apt install build-essential pkg-config libxkbcommon-dev libegl1-mesa-dev libwayland-dev libx11-dev
git clone https://github.com/CodeFu72/launchshelf.git
cd launchshelf
cargo build --locked --release
./target/release/launchshelf
```

Arch/Omarchy typically needs the `base-devel`, `pkgconf`, `libxkbcommon`, `mesa`,
`wayland` and `libx11` packages. These instructions need broader distribution testing.

Folder selection uses the desktop portal. Source/docs shortcuts need `xdg-open`;
desktop entry import needs `gtk-launch`. Terminal mode supports foot, kitty,
alacritty and xterm. Tools named in project commands must be installed separately.

Use `scripts/install.sh` after building to install the binary, icon and launcher
under your user account. Run `scripts/uninstall.sh` to remove that installation;
it preserves the library and project sources.

## Commands and storage

```sh
launchshelf --detect /path/to/project  # Preview detected settings
launchshelf --list                    # List saved projects
launchshelf --launch PROJECT_ID       # Run a saved launch command
launchshelf --build PROJECT_ID        # Run a saved build command
launchshelf --demo                    # Synthetic, non-persistent interface demo
```

The library lives in `$XDG_DATA_HOME/launchshelf`, falling back to
`~/.local/share/launchshelf`. Override it with `LAUNCHSHELF_DATA`. Source files stay
where they are; the library stores references. Logs may contain application output
and local paths, so review them before sharing an issue.

Commands use quoted executable/argument parsing with no automatic variable, tilde,
glob or shell expansion. Use absolute paths or commands on `PATH`. For pipelines,
explicitly configure a shell, such as `bash -lc 'first-command && second-command'`.
Terminal mode captures the terminal process's output; application output appears
inside that terminal and may not be captured in the LaunchShelf log.

Running direct GUI apps produces a running status until their process exits.
Desktop launch helpers may exit after submitting a launch request; their successful
exit does not guarantee the launched app stayed running. Closing LaunchShelf does
not kill launched apps. Git information refreshes when requested or the library
changes; it is not continuously monitored.

## Development

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

See [CONTRIBUTING.md](CONTRIBUTING.md) and [the development roadmap](docs/ROADMAP.md).
Personal registries and customer documents are not part of this repository.

## Community

Created by [Kirk Fuson / CodeFu72](https://github.com/CodeFu72) as a first
open-source project for the Linux community. Bug reports, documentation fixes,
project detection improvements and testing on other desktops are welcome.

- [Report a bug or suggest a feature](https://github.com/CodeFu72/launchshelf/issues)
- [Contribution guide](CONTRIBUTING.md)
- [Community conduct](CODE_OF_CONDUCT.md)
- [Security reporting](SECURITY.md)

Local validation: 11 core regression tests, formatting, Clippy, native rendering
and an isolated user-prefix install/uninstall check. GitHub Actions runs the Rust
checks for pushes and pull requests; check its results before relying on a commit.

Licensed under MIT. Omarchy is an optional integration; LaunchShelf does not
replace its launcher or alter packaged system configuration.
