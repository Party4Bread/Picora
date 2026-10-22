import tempfile
import unittest
from pathlib import Path
from update_manifest import create_manifest

class ManifestTests(unittest.TestCase):
    def test_complete_release_and_version(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for suffix in ['windows-x86_64.zip', 'linux-x86_64.tar.gz', 'macos-aarch64.zip']:
                (root / f'picora-0.2.0-{suffix}').write_bytes(b'build')
            manifest = create_manifest(root, '0.2.0', 'v0.2.0')
            self.assertEqual(len(manifest['targets']), 3)
            self.assertTrue(all(a['size'] == 5 and len(a['sha256']) == 64 for a in manifest['targets']))
            with self.assertRaises(ValueError): create_manifest(root, '0.2.0', 'v0.3.0')
            (root / 'picora-0.2.0-windows-x86_64.zip').unlink()
            with self.assertRaises(ValueError): create_manifest(root, '0.2.0', 'v0.2.0')

    def test_dev_manifest_is_complete_and_rejected_by_stable_publisher(self):
        version = '0.1.8-dev.123.gabcdef123456'
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for suffix in ['windows-x86_64.zip', 'linux-x86_64.tar.gz', 'macos-aarch64.zip']:
                (root / f'picora-{version}-{suffix}').write_bytes(b'dev build')
            with self.assertRaises(ValueError):
                create_manifest(root, version, f'v{version}')
            manifest = create_manifest(root, version, f'v{version}', dev=True)
            self.assertEqual(manifest['version'], version)
            self.assertEqual(len(manifest['targets']), 3)
            (root / f'picora-{version}-macos-aarch64.zip').unlink()
            with self.assertRaises(ValueError):
                create_manifest(root, version, f'v{version}', dev=True)

if __name__ == '__main__': unittest.main()
