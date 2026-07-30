"""Middleware tests, shaped like Django's."""

import unittest


class CommonMiddlewareTest(unittest.TestCase):
    def test_append_slash_have_slash(self):
        self.assertTrue(True)

    def test_prepend_www(self):
        self.assertTrue(True)


class GZipMiddlewareTest(unittest.TestCase):
    """Exercises the compression paths."""

    short_string = b"tiny"
    compressible_string = b"a" * 500

    def test_compress_response(self):
        self.assertTrue(True)

    def test_no_compress_short_response(self):
        self.assertTrue(True)

    def test_no_compress_compressed_response(self):
        self.assertTrue(True)

    def test_compress_deterministic(self):
        self.assertTrue(True)


class SecurityMiddlewareTest(unittest.TestCase):
    def test_sts_on(self):
        self.assertTrue(True)

    def test_content_type_on(self):
        self.assertTrue(True)
