"""Generate the updater contract only from a complete, tested release set."""
import argparse
import hashlib
import json
import re
from pathlib import Path

TARGETS = {
    'windows-x86_64': 'windows-x86_64.zip',
    'linux-x86_64': 'linux-x86_64.tar.gz',
    'macos-aarch64': 'macos-aarch64.zip',
}

def create_manifest(directory, version, tag, dev=False):
    pattern = r'(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)'
    if dev:
        pattern += r'-dev\.[1-9]\d*\.g[0-9a-f]{12}'
    if not re.fullmatch(pattern, version) or tag != f'v{version}':
        raise ValueError('Release tag must match the selected build version and channel')
    assets = []
    for target, suffix in TARGETS.items():
        name = f'picora-{version}-{suffix}'
        matches = list(directory.rglob(name))
        if len(matches) != 1 or not matches[0].is_file():
            raise ValueError(f'Expected exactly one {name}')
        path = matches[0]
        size = path.stat().st_size
        if not 0 < size <= 256 * 1024 * 1024:
            raise ValueError(f'Invalid archive size: {name}')
        with path.open('rb') as stream:
            digest = hashlib.file_digest(stream, 'sha256').hexdigest()
        assets.append(dict(target=target, name=name, size=size, sha256=digest))
    return dict(schema=1, version=version, targets=assets)

if __name__ == '__main__':
    import tomllib
    parser = argparse.ArgumentParser()
    parser.add_argument('--tag', required=True)
    parser.add_argument('--dev-version')
    parser.add_argument('--directory', type=Path, default=Path('dist'))
    args = parser.parse_args()
    version = tomllib.loads(Path('Cargo.toml').read_text())['package']['version']
    if args.dev_version:
        if not args.dev_version.startswith(version + '-dev.'):
            raise ValueError('Development version must match Cargo.toml')
        version = args.dev_version
    manifest = create_manifest(args.directory, version, args.tag, dev=bool(args.dev_version))
    (args.directory / 'update-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n')
