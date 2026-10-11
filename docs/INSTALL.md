# Install the Linux download

Download the **Linux x86_64 `.tar.gz`** archive and `SHA256SUMS` from
[the v0.1.0 release](https://github.com/CodeFu72/launchshelf/releases/tag/v0.1.0).
The automatic “Source code” downloads do not include the compiled app.

This is an early development release. Requires Linux x86_64, glibc 2.39 or newer,
Python 3.8 or newer, a Wayland or X11 session, and OpenGL/EGL. No Rust compiler,
FUSE or administrator privileges are needed to install the archive.

From the directory containing the downloaded files:

```sh
sha256sum -c SHA256SUMS
tar -xzf launchshelf-v0.1.0-linux-x86_64.tar.gz
cd launchshelf-v0.1.0-linux-x86_64
./install.sh
```

Then open **LaunchShelf** from your application launcher. The installer creates
`~/.local/bin/launchshelf`, a desktop entry and an icon. Library data and logs live
under `$XDG_DATA_HOME/launchshelf` or `~/.local/share/launchshelf`.

You can also run `./bin/launchshelf` directly from the extracted folder. It still
saves the library in your normal user data location.

## Desktop dependencies

The app dynamically loads graphics, window-system and keyboard libraries. On
Ubuntu 24.04, if these are missing:

```sh
sudo apt install python3 libegl1 libgl1 libxkbcommon0 libwayland-client0 libx11-6 xdg-desktop-portal xdg-utils libgtk-3-bin
```

On Arch/Omarchy, the corresponding packages are `python`, `mesa`, `libxkbcommon`,
`wayland`, `libx11`, `xdg-desktop-portal`, `xdg-utils` and `gtk3`. Folder selection
needs a portal backend suitable for your desktop. `gtk-launch` is used for desktop
app import; tools named in project commands must be installed separately.

The package is built on Ubuntu 24.04, checked there without a display and manually
checked on Omarchy. Other graphical desktop combinations are not yet verified.
Older distributions and ARM64 users should build from source.

## Upgrade and uninstall

To upgrade, extract the newer archive and run its `./install.sh`. The installer
preserves your library and logs, and refuses to overwrite unmanaged or modified
installation files. Keep the extracted package for its uninstall script.

```sh
./uninstall.sh
```

Uninstall removes the managed app, icon and desktop entry; it preserves library,
logs and project source folders. Optional custom installation for testing:
`./install.sh --prefix /absolute/path/to/prefix` (use the same prefix to uninstall).
