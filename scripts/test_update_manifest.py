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

if __name__ == '__main__': unittest.main()
