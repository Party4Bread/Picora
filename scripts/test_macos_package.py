"""Validate the shipped macOS archive, including its sealed app bundle."""
import argparse
import os
from pathlib import Path
import platform
import plistlib
import subprocess
import tempfile
import time
import zipfile


def verify(archive):
    with zipfile.ZipFile(archive) as z:
        assert z.testzip() is None, 'Corrupt ZIP'
        prefix = 'Picora/Picora.app/Contents/'
        info = plistlib.loads(z.read(prefix + 'Info.plist'))
        executable = prefix + 'MacOS/' + info['CFBundleExecutable']
        assert z.getinfo(executable).external_attr >> 16 & 0o111
        assert prefix + '_CodeSignature/CodeResources' in z.namelist(), 'App bundle is not sealed'
    if platform.system() != 'Darwin':
        return
    with tempfile.TemporaryDirectory(prefix='picora-package-test-') as tmp:
        root = Path(tmp)
        subprocess.run(['ditto', '-x', '-k', str(archive), str(root)], check=True)
        app = root / 'Picora/Picora.app'
        subprocess.run(['codesign', '--verify', '--deep', '--strict', '--verbose=2', str(app)], check=True)
        binary = app / 'Contents/MacOS' / info['CFBundleExecutable']
        env = dict(os.environ, PICORA_DATA_DIR=str(root / 'data'))
        with (root / 'launch.log').open('w+') as log:
            process = subprocess.Popen([str(binary), '--library'], env=env, stdout=log, stderr=log)
            try:
                time.sleep(8)
                log.seek(0)
                assert process.poll() is None, 'Packaged app exited at launch: ' + log.read()
            finally:
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('archive', type=Path)
    verify(parser.parse_args().archive)
