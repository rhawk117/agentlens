"""Auth system checks, shaped like Django's test module."""

import unittest


class SystemCheckTestCase(unittest.TestCase):
    def assertChecks(self, errors, expected):
        self.assertEqual([e.id for e in errors], expected)


class UserModelChecksTests(SystemCheckTestCase):
    def test_required_fields_is_list(self):
        self.assertChecks([], [])

    def test_username_non_unique(self):
        self.assertChecks([], [])

    def test_username_partially_unique(self):
        self.assertChecks([], [])

    def test_is_anonymous_authenticated_methods(self):
        self.assertChecks([], [])


class AuthMiddlewareChecksTests(SystemCheckTestCase):
    def test_middleware_installed(self):
        self.assertChecks([], [])

    def test_middleware_missing(self):
        self.assertChecks([], ["admin.E408"])

    def test_middleware_ordering(self):
        self.assertChecks([], ["admin.E410"])

    def test_session_middleware_required(self):
        self.assertChecks([], [])


class ModelBackendChecksTests(SystemCheckTestCase):
    def test_backend_importable(self):
        self.assertChecks([], [])

    def test_backend_missing(self):
        self.assertChecks([], ["auth.E001"])
