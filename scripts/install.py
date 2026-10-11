#!/usr/bin/env python3
"""Install or remove LaunchShelf under the current user's account."""
import argparse
import hashlib
import json
import os
import platform
import shutil
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--uninstall', action='store_true')
parser.add_argument('--prefix', type=Path, help='Custom prefix; defaults to ~/.local')
options = parser.parse_args()
project = Path(__file__).resolve().parents[1]
prefix = options.prefix.expanduser().resolve() if options.prefix else Path.home() / '.local'
data = prefix / 'share' if options.prefix else Path(os.environ.get('XDG_DATA_HOME', str(Path.home() / '.local/share')))
if not data.is_absolute():
    raise SystemExit('XDG_DATA_HOME must be an absolute path.')
binary = prefix / 'bin/launchshelf'
icon = data / 'icons/hicolor/scalable/apps/launchshelf.svg'
desktop = data / 'applications/launchshelf.desktop'
manifest = data / 'launchshelf/install-manifest.json'
locations = [binary, icon, desktop]

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def existing_installation():
    if not manifest.is_file():
        if any(p.exists() for p in locations):
            raise SystemExit('Unmanaged LaunchShelf files already exist; preserved.')
        return None
    try:
        owned = json.loads(manifest.read_text())
        if set(owned) != {str(p) for p in locations}:
            raise ValueError('Installation paths changed')
        for name, sha in owned.items():
            path = Path(name)
            if path.exists() and (path.is_symlink() or digest(path) != sha):
                raise ValueError('Installed file changed: ' + name)
        return owned
    except (ValueError, TypeError, OSError) as error:
        raise SystemExit(str(error) + '; existing files preserved.')

owned = existing_installation()
if options.uninstall:
    if owned is None:
        raise SystemExit('No managed LaunchShelf installation found.')
    for path in locations:
        path.unlink(missing_ok=True)
    manifest.unlink()
    print('LaunchShelf uninstalled. Library, project sources and logs were preserved.')
else:
    source = project / 'bin/launchshelf'
    if not source.is_file():
        source = project / 'target/release/launchshelf'
    if not source.is_file():
        raise SystemExit('No binary found. Source checkout: cargo build --locked --release')
    build_info = project / 'BUILDINFO.json'
    if build_info.is_file():
        info = json.loads(build_info.read_text())
        if platform.system() != 'Linux' or platform.machine() not in ['x86_64', 'AMD64']:
            raise SystemExit('This download requires Linux x86_64.')
        libc, version = platform.libc_ver()
        minimum = info['minimum_glibc']
        if libc != 'glibc' or tuple(map(int, version.split('.'))) < tuple(map(int, minimum.split('.'))):
            raise SystemExit('This download requires glibc ' + minimum + ' or newer. Build from source on older systems.')
    executable = str(binary)
    if any(c in executable for c in '\n\r'):
        raise SystemExit('Unsupported installation path')
    escaped = executable.replace('\\', '\\\\\\\\').replace('"', '\\"').replace('`', '\\`').replace('$', '\\$').replace('%', '%%')
    for path in [*locations, manifest]:
        path.parent.mkdir(parents=True, exist_ok=True)
    temporary = binary.with_name('launchshelf.installing')
    shutil.copy2(source, temporary)
    temporary.chmod(0o755)
    temporary.replace(binary)
    shutil.copy2(project / 'assets/launchshelf.svg', icon)
    desktop.write_text(f'''[Desktop Entry]
Type=Application
Name=LaunchShelf
Comment=A home for the apps you build
Exec="{escaped}"
Icon=launchshelf
Terminal=false
Categories=Development;
Keywords=projects;launcher;Rust;workshop;
StartupWMClass=launchshelf
X-LaunchShelf-Owned=true
''')
    temporary_manifest = manifest.with_suffix('.json.tmp')
    temporary_manifest.write_text(json.dumps({str(p): digest(p) for p in locations}, indent=2))
    temporary_manifest.replace(manifest)
    print('Installed LaunchShelf:', binary)
    print('Open LaunchShelf from your application launcher.')
    print('Library:', data / 'launchshelf')
