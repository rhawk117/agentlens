"""Request handling, shaped like Django's base handler."""

import asyncio
import logging

from django.core.handlers import exception
from django.utils.deprecation import MiddlewareMixin

logger = logging.getLogger("django.request")


class BaseHandler:
    """Builds the middleware chain and drives a request through it."""

    _view_middleware = None
    _template_response_middleware = None
    _exception_middleware = None
    _middleware_chain = None

    def load_middleware(self, is_async=False):
        """Populate middleware lists from settings.MIDDLEWARE."""
        self._view_middleware = []
        self._template_response_middleware = []
        self._exception_middleware = []
        handler = self._get_response_async if is_async else self._get_response
        for path in reversed(self.middleware_paths()):
            middleware = self.import_middleware(path)
            handler = exception.convert_exception_to_response(middleware(handler))
        self._middleware_chain = handler

    def middleware_paths(self):
        from django.conf import global_settings

        return list(global_settings.MIDDLEWARE)

    def import_middleware(self, path):
        raise NotImplementedError

    def _get_response(self, request):
        """Resolve and call the view, then apply response middleware."""
        response = None
        for middleware_method in self._view_middleware:
            response = middleware_method(request)
            if response:
                break
        if response is None:
            response = self.call_view(request)
        return response

    async def _get_response_async(self, request):
        """The async twin of _get_response."""
        response = None
        for middleware_method in self._view_middleware:
            response = await middleware_method(request)
            if response:
                break
        if response is None:
            response = await asyncio.sleep(0, self.call_view(request))
        return response

    def call_view(self, request):
        raise NotImplementedError

    def process_exception_by_middleware(self, exc, request):
        """Give each exception middleware a chance to handle exc."""
        for middleware_method in self._exception_middleware:
            response = middleware_method(request, exc)
            if response:
                return response
        raise exc

    def resolve_request(self, request):
        raise NotImplementedError

    def adapt_method_mode(self, is_async, method):
        return method

    def check_response(self, response, callback):
        if response is None:
            raise ValueError("view returned None")
        return response
