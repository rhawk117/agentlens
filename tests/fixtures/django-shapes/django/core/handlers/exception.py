"""Exception-to-response conversion."""

import logging

logger = logging.getLogger("django.request")


def convert_exception_to_response(get_response):
    """Wrap get_response so exceptions become responses, not tracebacks."""

    def inner(request):
        try:
            return get_response(request)
        except Exception as exc:
            return response_for_exception(request, exc)

    return inner


def response_for_exception(request, exc):
    """Map an exception onto the response it should produce."""
    if isinstance(exc, PermissionError):
        return handle_permission_denied(request, exc)
    if isinstance(exc, FileNotFoundError):
        return handle_not_found(request, exc)
    logger.exception("internal server error", exc_info=exc)
    return handle_server_error(request, exc)


def handle_permission_denied(request, exc):
    return {"status": 403}


def handle_not_found(request, exc):
    return {"status": 404}


def handle_server_error(request, exc):
    return {"status": 500}


def get_exception_response(request, resolver, status_code, exception):
    return {"status": status_code}
