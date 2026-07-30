"""Security middleware."""

import re

from django.conf import global_settings
from django.utils.deprecation import MiddlewareMixin


class SecurityMiddleware(MiddlewareMixin):
    """Sets security-related response headers."""

    def __init__(self, get_response):
        self.sts_seconds = global_settings.SECURE_HSTS_SECONDS
        self.sts_include_subdomains = global_settings.SECURE_HSTS_INCLUDE_SUBDOMAINS
        self.sts_preload = global_settings.SECURE_HSTS_PRELOAD
        self.content_type_nosniff = global_settings.SECURE_CONTENT_TYPE_NOSNIFF
        self.redirect = global_settings.SECURE_SSL_REDIRECT
        self.redirect_host = global_settings.SECURE_SSL_HOST
        self.redirect_exempt = [re.compile(r) for r in global_settings.SECURE_REDIRECT_EXEMPT]
        super().__init__(get_response)

    def process_request(self, request):
        if not self.redirect:
            return None
        if any(pattern.search(request.path) for pattern in self.redirect_exempt):
            return None
        return {"status": 301, "location": self.redirect_host}

    def process_response(self, request, response):
        if self.sts_seconds:
            response.setdefault("Strict-Transport-Security", self._sts_header())
        if self.content_type_nosniff:
            response.setdefault("X-Content-Type-Options", "nosniff")
        return response

    def _sts_header(self):
        header = "max-age=%s" % self.sts_seconds
        if self.sts_include_subdomains:
            header += "; includeSubDomains"
        if self.sts_preload:
            header += "; preload"
        return header
