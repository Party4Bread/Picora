import unittest
from prune_dev_releases import superseded

def release(tag, prerelease=True):
    return {'tag_name': tag, 'prerelease': prerelease}

class PruneTests(unittest.TestCase):
    def test_only_older_dev_prereleases_are_removed(self):
        releases = [
            release('v0.1.10-dev.300.g0123456789ab'),
            release('v0.1.10', prerelease=False),
            release('v0.1.9-dev.200.gabcdef123456'),
            release('v0.1.9', prerelease=False),
            release('v0.1.8-dev.100.gabcdef123456'),
            release('v0.1.9-dev.150.gabcdef123456', prerelease=False),
            release('v0.1.9-beta.1'),
            release('v0.1.9-dev.1.gnothexhash00'),
        ]
        self.assertEqual(superseded(releases, 'v0.1.10'),
                         ['v0.1.9-dev.200.gabcdef123456', 'v0.1.8-dev.100.gabcdef123456'])
        self.assertEqual(superseded(releases, 'v0.1.8'), [])

    def test_rejects_non_stable_tags(self):
        for tag in ['0.1.10', 'v0.1.10-dev.1.gabcdef123456', 'v01.1.0']:
            with self.assertRaises(ValueError):
                superseded([], tag)

if __name__ == '__main__': unittest.main()
