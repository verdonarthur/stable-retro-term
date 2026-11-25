import socket
import time
import unittest

from metrics import compare, drm_client, gpu_delta, summarize
from run import Fixture, WorkloadError, assert_frame, matching
from wayland import DisplayTimeout, visible


class Counters(unittest.TestCase):
    def test_drm_identity_and_engine_time(self):
        def parse(body):
            return drm_client(
                "drm-driver:\tamdgpu\ndrm-pdev:\t0000:63:00.0\ndrm-client-id:\t19\n" + body
            )

        self.assertEqual(parse("drm-engine-gfx:\t91 ns\ndrm-engine-capacity-gfx:\t2\n"),
                         ("amdgpu:0000:63:00.0:19", {"gfx": 91}))
        with self.assertRaises(ValueError):
            parse("drm-engine-gfx:\t91 cycles\n")

    def test_temporary_decrease_and_client_loss(self):
        client = "amdgpu:0000:63:00.0:19"

        def counters(*values):
            return [{"drm": {client: {"gfx": value}} if value is not None else {}}
                    for value in values]

        self.assertEqual(gpu_delta(counters(100, 99, 113), False)[0], 13)
        self.assertEqual(gpu_delta(counters(100, 97), False), (None, None))
        self.assertEqual(gpu_delta(counters(100, None, 113), False), (None, None))
        self.assertEqual(gpu_delta(counters(None, 4, 13), True)[0], 13)
        self.assertEqual(gpu_delta(counters(None, 4, 13), False), (None, None))

    def test_engine_reported_after_first_use(self):
        client = "amdgpu:0000:63:00.0:19"
        samples = [{"drm": {client: engines}} for engines in ({}, {}, {"gfx": 7})]
        self.assertEqual(gpu_delta(samples[:2], False), (None, None))
        self.assertEqual(gpu_delta(samples, False), (7, {"amdgpu:0000:63:00.0": {"gfx": 7}}))

    def test_resource_gate(self):
        def pair(a, b, valid=True):
            def result(value):
                return {"valid": valid, "metrics": {"gpu_ns": value},
                        "engines": {"amdgpu:0000:63:00.0": {"gfx": value}}}

            return {"candidate": result(a), "baseline": result(b)}

        cases = [
            ([pair(140, 100)], 0, "passed"),
            ([pair(141, 100)], 0, "failed"),
            ([pair(0, 0)], 0, "passed"),
            ([pair(1, 0)], 0, "failed"),
            ([pair(0, 0)], 1, "unverified"),
            ([pair(138, 100), pair(143, 100)], 1, "unverified"),
            ([pair(0, 1)], 1, "unverified"),
            ([pair(None, 100)], 1, "unverified"),
            ([pair(1, 100, False)], 1, "unverified"),
            ([pair(130, 100), pair(190, 100), pair(135, 100)], 0, "passed"),
            ([pair(150, 100), pair(90, 100), pair(145, 100)], 0, "failed"),
            ([pair(130, 100), pair(150, 100)], 0, "unverified"),
        ]
        for pairs, resolution, expected in cases:
            with self.subTest(pairs=pairs, resolution=resolution):
                self.assertEqual(compare(pairs, "gpu_ns", resolution)["status"], expected)

    def test_negligible_cpu_passes(self):
        def pair(a, b):
            def result(value):
                return {"valid": True, "metrics": {"cpu_ns": value}}

            return {"candidate": result(a), "baseline": result(b)}

        self.assertEqual(compare([pair(75_000, 18_000)], "cpu_ns")["status"], "passed")
        self.assertEqual(compare([pair(1_500_000, 900_000)], "cpu_ns")["status"], "failed")

    def test_incomplete_sampling_window(self):
        self.assertEqual(summarize([{}], complete=False)[0]["cpu_ns"], None)

    def test_cpu_and_rss_are_independent_of_gpu(self):
        candidate = {"valid": True, "metrics": {
            "cpu_ns": 80_000_000, "steady_rss_bytes": 2_000_000, "gpu_ns": None,
        }}
        baseline = {"valid": True, "metrics": {
            "cpu_ns": 2_000_000_000, "steady_rss_bytes": 1_000_000, "gpu_ns": None,
        }}
        pairs = [{"candidate": candidate, "baseline": baseline}]
        self.assertEqual(compare(pairs, "cpu_ns")["status"], "passed")
        self.assertEqual(compare(pairs, "steady_rss_bytes")["status"], "failed")
        self.assertEqual(compare(pairs, "gpu_ns")["status"], "unverified")


class Display(unittest.TestCase):
    def test_grid_boundaries_and_cursor(self):
        width, height, stride, offset = 44, 36, 184, 8
        image = bytearray(offset + stride * height)
        color = [37, 83, 121]
        pixel = bytes((121, 83, 37, 255))
        for row in range(3, 27):
            start = offset + row * stride + 4 * 4
            image[start:start + 4 * 32] = pixel * 32
        cursor = offset + (3 + 6) * stride + (4 + 6) * 4
        image[cursor:cursor + 4] = bytes((255, 255, 255, 255))
        for col in range(5):
            glyph = offset + (3 + 9) * stride + (4 + 4 * col + 1) * 4
            image[glyph:glyph + 4] = bytes((255, 255, 255, 255))
        frame = width, height, stride, offset, bytes(image)
        self.assertEqual(visible(frame, color, [6, 8, 32, 24]),
                         {"area": [4, 3, 32, 24], "cursor": True})
        self.assertIsNone(visible(frame, [1, 2, 3], [6, 8, 32, 24]))
        self.assertIsNone(visible(frame, color, [7, 8, 32, 28]))

    def test_missing_and_late_frames(self):
        class Missing:
            def observe(self, color, grid, timeout):
                raise DisplayTimeout("capture did not contain the marker")

        with self.assertRaises(DisplayTimeout):
            assert_frame(Missing(), [11, 22, 33], [6, 8, 32, 24], 1, time.monotonic_ns())
        with self.assertRaises(WorkloadError):
            assert_frame(Missing(), [11, 22, 33], [6, 8, 32, 24], 0, time.monotonic_ns())

    def test_grid_mismatch_invalidates_both_legs(self):
        pair = {
            "candidate": {"valid": True, "evidence": [{"grid": [26, 100, 800, 494]}]},
            "baseline": {"valid": True, "evidence": [{"grid": [26, 99, 792, 494]}]},
        }
        matching(pair)
        self.assertFalse(pair["candidate"]["valid"])
        self.assertFalse(pair["baseline"]["valid"])

    def test_early_fixture_exit_is_a_failed_workload(self):
        parent, child = socket.socketpair()
        fixture = Fixture(parent)
        try:
            child.close()
            with self.assertRaises(WorkloadError):
                fixture.receive()
        finally:
            fixture.close()


if __name__ == "__main__":
    unittest.main()
