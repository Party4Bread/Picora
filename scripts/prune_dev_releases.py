"""Delete development prereleases built for versions older than a new stable release."""
import argparse
import json
import os
import re
import subprocess

DEV_TAG = re.compile(r'v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)-dev\.[1-9]\d*\.g[0-9a-f]{12}')
STABLE_TAG = re.compile(r'v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)')


def superseded(releases, stable_tag):
    """Dev prerelease tags whose base version is lower than `stable_tag`.

    Builds of the released version itself stay, so the Dev channel keeps the
    build from the release commit until newer main builds replace it.
    """
    stable = STABLE_TAG.fullmatch(stable_tag)
    if stable is None:
        raise ValueError(f'Not a stable release tag: {stable_tag}')
    released = tuple(map(int, stable.groups()))
    tags = []
    for release in releases:
        dev = DEV_TAG.fullmatch(release['tag_name'])
        if release['prerelease'] and dev and tuple(map(int, dev.groups())) < released:
            tags.append(release['tag_name'])
    return tags


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--tag', required=True)
    parser.add_argument('--repo', default=os.environ.get('GITHUB_REPOSITORY'))
    args = parser.parse_args()
    listing = subprocess.run(
        ['gh', 'api', '--paginate', '--slurp', f'repos/{args.repo}/releases?per_page=100'],
        capture_output=True, text=True, check=True)
    releases = [release for page in json.loads(listing.stdout) for release in page]
    for tag in superseded(releases, args.tag):
        print(f'Deleting superseded development build {tag}')
        subprocess.run(['gh', 'release', 'delete', tag, '--repo', args.repo, '--yes', '--cleanup-tag'],
                       check=True)


if __name__ == '__main__':
    main()
