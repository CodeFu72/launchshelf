#!/usr/bin/env python3
"""User-local installation; never modifies system directories."""
import argparse, hashlib, json, os, shutil
from pathlib import Path

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--uninstall',action='store_true')
parser.add_argument('--prefix',type=Path,help='Custom installation prefix; defaults to ~/.local')
options=parser.parse_args()
project=Path(__file__).resolve().parents[1]
home=Path.home()
prefix=options.prefix.expanduser().resolve() if options.prefix else home/'.local'
data=prefix/'share' if options.prefix else Path(os.environ.get('XDG_DATA_HOME',str(home/'.local/share')))
binary=prefix/'bin/launchshelf'
icon=data/'icons/hicolor/scalable/apps/launchshelf.svg'
desktop=data/'applications/launchshelf.desktop'
manifest=data/'launchshelf/install-manifest.json'
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
if options.uninstall:
    if not manifest.is_file():raise SystemExit('No managed LaunchShelf installation found.')
    owned=json.loads(manifest.read_text())
    expected={str(binary),str(icon),str(desktop)}
    if set(owned)!=expected:raise SystemExit('Installation locations changed; original files preserved.')
    for name,sha in owned.items():
        path=Path(name)
        if path.exists() and digest(path)!=sha:raise SystemExit('Installed file changed; preserved: '+name)
    for name in owned:Path(name).unlink(missing_ok=True)
    manifest.unlink()
    print('LaunchShelf uninstalled. Library, project sources and logs were preserved.')
else:
    source=project/'target/release/launchshelf'
    if not source.is_file():raise SystemExit('Build first: cargo build --locked --release')
    if any(path.exists() for path in [binary,icon,desktop]) and not manifest.is_file():raise SystemExit('An unmanaged LaunchShelf installation file already exists; preserved.')
    for path in [binary,icon,desktop,manifest]:path.parent.mkdir(parents=True,exist_ok=True)
    temporary=binary.with_name('launchshelf.installing')
    shutil.copy2(source,temporary)
    temporary.chmod(0o755)
    temporary.replace(binary)
    shutil.copy2(project/'assets/launchshelf.svg',icon)
    executable=str(binary)
    if any(c in executable for c in '\n\r'):raise SystemExit('Unsupported installation path')
    escaped=executable.replace('\\','\\\\\\\\').replace('"','\\"').replace('`','\\`').replace('$','\\$').replace('%','%%')
    desktop.write_text(f'''[Desktop Entry]
Type=Application
Name=LaunchShelf
Comment=A home for the apps you build
Exec="{escaped}"
Icon=launchshelf
Terminal=false
Categories=Development;Utility;
Keywords=projects;launcher;Rust;workshop;
StartupWMClass=launchshelf
X-LaunchShelf-Owned=true
''')
    manifest.write_text(json.dumps({str(p):digest(p) for p in [binary,icon,desktop]},indent=2))
    print('Installed LaunchShelf:',binary)
    print('Library:',data/'launchshelf')
