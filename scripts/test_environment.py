"""Check panorama orientation against Godot's world-space sky coordinates."""
import unittest

import numpy as np

from environment import sample


class PanoramaTest(unittest.TestCase):
    def test_bevy_cube_lookup_matches_godot_world_axes(self):
        columns = np.arange(8, dtype='f4')
        hdr = np.tile(np.stack((columns, columns**2, columns*0), axis=-1), (2, 1, 1))
        # Godot maps -Z, +X, +Z, -X to panorama U = 0, 1/4, 1/2, 3/4.
        world = np.array([[0, 0, -1], [1, 0, 0], [0, 0, 1], [-1, 0, 0]], dtype='f4')
        # Bevy's skybox and IBL shaders negate world Z for cube lookup.
        cube = world * [1, 1, -1]
        expected = [[3.5, 24.5, 0], [1.5, 2.5, 0], [3.5, 12.5, 0], [5.5, 30.5, 0]]
        np.testing.assert_allclose(sample(hdr, cube), expected, atol=1e-5)


if __name__ == '__main__':
    unittest.main()
