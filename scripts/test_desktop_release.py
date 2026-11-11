import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('release', Path(__file__).with_name('desktop-release.py'))
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class DesktopReleaseTests(unittest.TestCase):
    def fixtures(self, root):
        output = root / 'assets'
        for platform, (_, formats) in release.PLATFORMS.items():
            bundle = root / platform
            for extension, pattern in formats.items():
                path = bundle / pattern.replace('*', 'Herdstead_1.2.3')
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b'installer')
                if extension in ('AppImage', 'app.tar.gz'):
                    Path(str(path) + '.sig').write_text('test-signature')
            release.stage(platform, bundle, output)
        return output

    def test_manifest_covers_every_architecture_and_uploaded_file(self):
        with tempfile.TemporaryDirectory() as directory:
            output = self.fixtures(Path(directory))
            release.assemble(output, 'v1.2.3', 'owner/repo')
            manifest = json.loads((output / 'latest.json').read_text())
            self.assertEqual(manifest['version'], '1.2.3')
            self.assertEqual(set(manifest['platforms']), {'darwin-aarch64', 'darwin-x86_64', 'linux-aarch64', 'linux-x86_64'})
            for entry in manifest['platforms'].values():
                self.assertTrue((output / entry['url'].rsplit('/', 1)[1]).is_file())
                self.assertEqual(entry['signature'], 'test-signature')
            subprocess.run(['shasum', '-a', '256', '-c', 'SHA256SUMS'], cwd=output, check=True, capture_output=True)

    def test_missing_platform_cannot_publish(self):
        with tempfile.TemporaryDirectory() as directory:
            output = self.fixtures(Path(directory))
            (output / 'Herdstead-linux-arm64.AppImage').unlink()
            with self.assertRaisesRegex(ValueError, 'missing or empty installer'):
                release.assemble(output, 'v1.2.3', 'owner/repo')
            self.assertFalse((output / 'latest.json').exists())

    def test_missing_signature_cannot_publish(self):
        with tempfile.TemporaryDirectory() as directory:
            output = self.fixtures(Path(directory))
            (output / 'Herdstead-macos-x64.app.tar.gz.sig').write_text('')
            with self.assertRaisesRegex(ValueError, 'empty signature'):
                release.assemble(output, 'v1.2.3', 'owner/repo')

    def test_staging_requires_signature_and_unique_bundle(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            image = root / 'bundle/appimage/test.AppImage'
            image.parent.mkdir(parents=True)
            image.write_bytes(b'installer')
            with self.assertRaises(FileNotFoundError):
                release.stage('linux-x64', root / 'bundle', root / 'assets')
            (image.parent / 'other.AppImage').write_bytes(b'installer')
            with self.assertRaisesRegex(ValueError, 'exactly one'):
                release.stage('linux-x64', root / 'bundle', root / 'assets')

    def test_version_stamp_updates_every_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = Path(__file__).resolve().parent.parent
            for name in ['scripts/set-version.py', 'Cargo.toml', 'crates/desktop/package.json', 'crates/desktop/native/tauri.conf.json']:
                destination = root / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes((source / name).read_bytes())
            subprocess.run(['python3', str(root / 'scripts/set-version.py'), 'v1.2.3'], check=True, capture_output=True)
            for name in ['crates/desktop/package.json', 'crates/desktop/native/tauri.conf.json']:
                self.assertEqual(json.loads((root / name).read_text())['version'], '1.2.3')
            for name in ['Cargo.toml']:
                self.assertIn('version = "1.2.3"', (root / name).read_text())
            failed = subprocess.run(['python3', str(root / 'scripts/set-version.py'), 'bad-tag'], capture_output=True)
            self.assertNotEqual(failed.returncode, 0)
