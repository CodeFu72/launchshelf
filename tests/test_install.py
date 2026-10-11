"""User-prefix installer behavior, without touching the real user installation."""
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

INSTALLER = Path(__file__).resolve().parents[1] / 'scripts/install.py'

class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.package = self.root / 'package'
        (self.package / 'scripts').mkdir(parents=True)
        (self.package / 'bin').mkdir()
        (self.package / 'assets').mkdir()
        shutil.copy2(INSTALLER, self.package / 'scripts/install.py')
        (self.package / 'assets/launchshelf.svg').write_text('<svg/>')
        (self.package / 'bin/launchshelf').write_text('#!/bin/sh\nexit 0\n')
        self.prefix = self.root / 'prefix with spaces %'

    def tearDown(self):
        self.temp.cleanup()

    def run_installer(self, *args):
        return subprocess.run([sys.executable, str(self.package / 'scripts/install.py'), '--prefix', str(self.prefix), *args], capture_output=True, text=True)

    def test_install_upgrade_uninstall_preserves_library(self):
        self.assertEqual(self.run_installer().returncode, 0)
        binary = self.prefix / 'bin/launchshelf'
        self.assertEqual(subprocess.run([str(binary)]).returncode, 0)
        desktop = self.prefix / 'share/applications/launchshelf.desktop'
        self.assertIn('prefix with spaces %%', desktop.read_text())
        if shutil.which('desktop-file-validate'):
            self.assertEqual(subprocess.run(['desktop-file-validate', str(desktop)]).returncode, 0)
        state = self.prefix / 'share/launchshelf/projects.json'
        state.write_text('{"private":"preserve"}')
        log = state.parent / 'demo.log'
        log.write_text('retained output')
        (self.package / 'bin/launchshelf').write_text('#!/bin/sh\nexit 7\n')
        self.assertEqual(self.run_installer().returncode, 0)
        self.assertEqual(subprocess.run([str(binary)]).returncode, 7)
        self.assertEqual(self.run_installer('--uninstall').returncode, 0)
        self.assertFalse(binary.exists())
        self.assertFalse(desktop.exists())
        self.assertEqual(json.loads(state.read_text()), {'private': 'preserve'})
        self.assertEqual(log.read_text(), 'retained output')

    def test_unmanaged_files_are_not_overwritten(self):
        binary = self.prefix / 'bin/launchshelf'
        binary.parent.mkdir(parents=True)
        binary.write_text('existing user file')
        self.assertNotEqual(self.run_installer().returncode, 0)
        self.assertEqual(binary.read_text(), 'existing user file')

    def test_modified_installation_is_preserved_for_upgrade_and_uninstall(self):
        self.assertEqual(self.run_installer().returncode, 0)
        binary = self.prefix / 'bin/launchshelf'
        binary.write_text('modified by user')
        self.assertNotEqual(self.run_installer().returncode, 0)
        self.assertNotEqual(self.run_installer('--uninstall').returncode, 0)
        self.assertEqual(binary.read_text(), 'modified by user')

    def test_changed_manifest_locations_cannot_delete_other_files(self):
        self.assertEqual(self.run_installer().returncode, 0)
        other = self.root / 'unrelated.txt'
        other.write_text('keep')
        manifest = self.prefix / 'share/launchshelf/install-manifest.json'
        manifest.write_text(json.dumps({str(other): 'irrelevant'}))
        self.assertNotEqual(self.run_installer('--uninstall').returncode, 0)
        self.assertEqual(other.read_text(), 'keep')

if __name__ == '__main__':
    unittest.main()
