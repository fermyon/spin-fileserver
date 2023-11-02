import { componentize } from "@bytecodealliance/componentize-js"
import { readFile, writeFile } from "node:fs/promises"

const { component } = await componentize(
    await readFile("app.mjs"),
    {
        witPath: "../wit",
        worldName: "proxy",
        preview2Adapter: "../../adapters/fd1e948d/wasi_snapshot_preview1.reactor.wasm",
        enableStdout: true,
    }
);

await writeFile("http.wasm", component)
