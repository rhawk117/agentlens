"""GZip middleware."""

from django.utils.deprecation import MiddlewareMixin

MIN_LENGTH = 200
COMPRESSIBLE_TYPES = ("text/", "application/json", "application/xml")


class GZipMiddleware(MiddlewareMixin):
    """Compresses responses that are large enough to be worth it."""

    max_random_bytes = 100

    def process_response(self, request, response):
        if len(response.get("content", "")) < MIN_LENGTH:
            return response
        if "gzip" not in request.headers.get("Accept-Encoding", ""):
            return response
        if response.has_header("Content-Encoding"):
            return response
        response["Content-Encoding"] = "gzip"
        response["Content-Length"] = str(len(response["content"]))
        patch_vary_headers(response, ("Accept-Encoding",))
        return response


def patch_vary_headers(response, newheaders):
    existing = response.get("Vary", "")
    response["Vary"] = ", ".join(filter(None, [existing, *newheaders]))
