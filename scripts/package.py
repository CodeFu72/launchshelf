#!/usr/bin/env python3
"""Package an already built Linux x86_64 executable; include only public files."""
import argparse
import gzip
import hashlib
import json
import re
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
from pathlib import Path

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--binary', type=Path, default=root / 'target/release/launchshelf')
parser.add_argument('--output', type=Path, default=root / 'dist')
args = parser.parse_args()
binary = args.binary.resolve()
if not binary.is_file():
    raise SystemExit('Build first: cargo build --locked --release')
header = subprocess.check_output(['readelf', '-h', str(binary)], text=True)
if 'Advanced Micro Devices X86-64' not in header:
    raise SystemExit('This packager supports Linux x86_64 only.')
versions = subprocess.check_output(['readelf', '--version-info', str(binary)], text=True)
required = re.findall(r'GLIBC_([0-9.]+)', versions)
minimum = max(required, key=lambda s: tuple(map(int, s.split('.'))))
version = tomllib.loads((root / 'Cargo.toml').read_text())['package']['version']
name = f'launchshelf-v{version}-linux-x86_64'
commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
epoch = int(subprocess.check_output(['git', 'show', '-s', '--format=%ct', 'HEAD'], cwd=root, text=True))
metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version', '1'], cwd=root))
args.output.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix='launchshelf-package-') as temporary:
    package = Path(temporary) / name
    (package / 'bin').mkdir(parents=True)
    shutil.copy2(binary, package / 'bin/launchshelf')
    (package / 'bin/launchshelf').chmod(0o755)
    for relative in ['assets/launchshelf.svg', 'scripts/install.py', 'LICENSE', 'README.md', 'docs/INSTALL.md']:
        destination = package / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(root / relative, destination)
    for filename, flag in [('install.sh', ''), ('uninstall.sh', '--uninstall')]:
        script = package / filename
        script.write_text('#!/bin/sh\nset -eu\npackage_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)\nexec python3 "$package_dir/scripts/install.py" ' + flag + ' "$@"\n')
        script.chmod(0o755)
    licenses = package / 'third-party-licenses'
    shutil.copytree(root / 'assets/third-party', licenses / 'upstream')
    notices = ['# Third-party notices', '', 'Dependency license expressions and upstream-supplied notices.', 'Includes platform-specific and build dependencies for completeness.', '', '| Package | Version | License expression |', '| --- | --- | --- |']
    for dependency in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
        if dependency['source'] is None:
            continue
        notices.append(f"| {dependency['name']} | {dependency['version']} | {dependency.get('license') or 'See upstream notices'} |")
        directory = Path(dependency['manifest_path']).parent
        for file in sorted(directory.rglob('*')):
            low = file.name.lower()
            is_notice = any(word in low for word in ['license', 'licence', 'copying', 'notice', 'ofl', 'ufl']) or (file.parent.name == 'fonts' and file.suffix == '.txt')
            if not file.is_file() or file.is_symlink() or not is_notice:
                continue
            destination = licenses / (dependency['name'] + '-' + dependency['version']) / file.relative_to(directory)
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(file, destination)
    sysroot = Path(subprocess.check_output(['rustc', '--print', 'sysroot'], text=True).strip())
    rust_notices = sysroot / 'share/doc/rust'
    for filename in ['COPYRIGHT.html', 'COPYRIGHT-library.html', 'LICENSE-MIT', 'LICENSE-APACHE']:
        source = rust_notices / filename
        if source.is_file():
            destination = licenses / 'rust' / filename
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
    (package / 'THIRD-PARTY-NOTICES.md').write_text('\n'.join(notices) + '\n')
    info = {'version': version, 'commit': commit, 'architecture': 'x86_64', 'minimum_glibc': minimum, 'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(), 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}
    (package / 'BUILDINFO.json').write_text(json.dumps(info, indent=2) + '\n')
    archive = args.output / (name + '.tar.gz')
    with archive.open('wb') as output:
        with gzip.GzipFile(filename='', mode='wb', fileobj=output, mtime=epoch) as compressed:
            with tarfile.open(fileobj=compressed, mode='w', format=tarfile.PAX_FORMAT) as tar:
                def normalize(member):
                    member.uid = member.gid = 0
                    member.uname = member.gname = ''
                    member.mtime = epoch
                    return member
                for file in [package, *sorted(package.rglob('*'))]:
                    tar.add(file, arcname=str(file.relative_to(package.parent)), recursive=False, filter=normalize)
checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
(args.output / 'SHA256SUMS').write_text(checksum + '  ' + archive.name + '\n')
print('Package:', archive)
print('Minimum glibc:', minimum)
