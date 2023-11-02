import { handle as spinFileserverHandle } from "wasi:http/incoming-handler@0.2.0-rc-2023-10-18"
import { OutgoingResponse, ResponseOutparam, OutgoingBody, Fields } from "wasi:http/types@0.2.0-rc-2023-10-18"

const encoder = new TextEncoder()
const disposeSymbol = Symbol.dispose || Symbol.for('dispose')

function handle(request, responseOut) {
    const method = request.method()
    const path = request.pathWithQuery()

    if (method.tag === "get") {
        if (path === "/hello") {
            const response = new OutgoingResponse(
                200,
                new Fields([["content-type", encoder.encode("text/plain")]])
            )
            
            const responseBody = response.write()
            ResponseOutparam.set(responseOut, { tag: "ok", val: response })
            
            const responseStream = responseBody.write()
            responseStream.blockingWriteAndFlush(encoder.encode("Hello, world!"))
            responseStream[disposeSymbol]()
            OutgoingBody.finish(responseBody)
        } else {
            spinFileserverHandle(request, responseOut)
        }
    } else {
        const response = new OutgoingResponse(400, new Fields([]))
        ResponseOutparam.set(responseOut, { tag: "ok", val: response })
        OutgoingBody.finish(response.write())
    }
}

export const incomingHandler = { handle }
