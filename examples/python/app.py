import asyncio
import hashlib
import poll_loop

from proxy import exports
from proxy.types import Ok
from proxy.imports import types, incoming_handler
from proxy.imports.types import (
    MethodGet, MethodPost, Scheme, SchemeHttp, SchemeHttps, SchemeOther, IncomingRequest, ResponseOutparam,
    OutgoingResponse, Fields, OutgoingBody, OutgoingRequest
)
from poll_loop import Stream, Sink, PollLoop

class IncomingHandler(exports.IncomingHandler):
    def handle(self, request: IncomingRequest, response_out: ResponseOutparam):
        # Dispatch the request using `asyncio`, backed by a custom event loop
        # based on `wasi:io/poll#poll-list`.
        loop = PollLoop()
        asyncio.set_event_loop(loop)
        loop.run_until_complete(handle_async(request, response_out))

async def handle_async(request: IncomingRequest, response_out: ResponseOutparam):
    method = request.method()
    path = request.path_with_query()

    if isinstance(method, MethodGet) and path == "/hello":
        response = OutgoingResponse(200, Fields([("content-type", b"text/plain")]))
        response_body = response.write()
        
        ResponseOutparam.set(response_out, Ok(response))

        sink = Sink(response_body)
        await sink.send(b"Hello, world!")
        sink.close()
        
    elif isinstance(method, MethodGet):
        # Delegate to spin-fileserver component.
        incoming_handler.handle(request, response_out)

    else:
        response = OutgoingResponse(400, Fields([]))
        ResponseOutparam.set(response_out, Ok(response))
        OutgoingBody.finish(response.write(), None)
        
