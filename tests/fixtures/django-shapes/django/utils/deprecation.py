"""Deprecation helpers, including the middleware adapter."""

import asyncio
import inspect
import warnings


class RemovedInNextVersionWarning(DeprecationWarning):
    pass


class MiddlewareMixin:
    """Adapts an old-style middleware class to the new callable protocol."""

    sync_capable = True
    async_capable = True

    def __init__(self, get_response):
        if get_response is None:
            raise ValueError("get_response must not be None")
        self.get_response = get_response
        self._async_check()
        super().__init__()

    def _async_check(self):
        if asyncio.iscoroutinefunction(self.get_response):
            self._is_coroutine = True

    def __call__(self, request):
        response = None
        if hasattr(self, "process_request"):
            response = self.process_request(request)
        response = response or self.get_response(request)
        if hasattr(self, "process_response"):
            response = self.process_response(request, response)
        return response

    async def __acall__(self, request):
        response = None
        if hasattr(self, "process_request"):
            response = await self.process_request(request)
        response = response or await self.get_response(request)
        if hasattr(self, "process_response"):
            response = await self.process_response(request, response)
        return response
