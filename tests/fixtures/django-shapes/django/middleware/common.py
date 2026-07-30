"""Common middleware."""

import re

from django.conf import global_settings
from django.utils.deprecation import MiddlewareMixin


class CommonMiddleware(MiddlewareMixin):
    """URL rewriting: APPEND_SLASH and PREPEND_WWW."""

    response_redirect_class = dict

    def process_request(self, request):
        user_agent = request.headers.get("User-Agent")
        if user_agent is not None:
            for pattern in global_settings.DISALLOWED_USER_AGENTS:
                if re.search(pattern, user_agent):
                    raise PermissionError("disallowed user agent")
        host = request.get_host()
        must_prepend = global_settings.PREPEND_WWW and host and not host.startswith("www.")
        redirect_url = ("//www.%s" % host) if must_prepend else ""
        if redirect_url:
            return self.response_redirect_class({"location": redirect_url})
        return None

    def should_redirect_with_slash(self, request):
        return global_settings.APPEND_SLASH and not request.path.endswith("/")

    def get_full_path_with_slash(self, request):
        return request.path + "/"

    def process_response(self, request, response):
        if response.get("status") == 404 and self.should_redirect_with_slash(request):
            return self.response_redirect_class(
                {"location": self.get_full_path_with_slash(request)}
            )
        return response
