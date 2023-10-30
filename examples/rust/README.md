# spin-fileserver-example

This is an example of using [component
composition](https://component-model.bytecodealliance.org/creating-and-consuming/composing.html)
to compose an example app with
[spin-fileserver](https://github.com/fermyon/spin-fileserver), a reusable
component for serving static files.

## Prerequisites

- [Rust](https://rustup.rs/), including the `wasm32-wasi` target
- [cargo-component](https://github.com/bytecodealliance/cargo-component)
- [wasm-tools](https://github.com/bytecodealliance/wasm-tools/)
  - Note that you'll need [this fork](https://github.com/dicej/wasm-tools/tree/wasm-compose-resource-imports) until [this PR](https://github.com/bytecodealliance/wasm-tools/pull/1261) has been merged and released.
- [curl](https://curl.se/download.html) or a web browser for testing
  
Once you have Rust installed, the following should give you everything else:

```shell
rustup target add wasm32-wasi
cargo install cargo-component
cargo install --locked --git https://github.com/dicej/wasm-tools \
    --branch wasm-compose-resource-imports wasm-tools
```

## Building and Running

To build and run the example, run:

```shell
spin build -u
```

Then, in another terminal, you can test it using `curl`:

```shell
curl -i http://127.0.0.1:3000/hello
```

The above should return a response body `Hello, world!`, served up by the
example app itself.  All other URIs are handled by `spin-fileserver`, e.g.:

```shell
curl -i http://127.0.0.1:3000/foo.txt
```

```shell
curl -i http://127.0.0.1:3000/nonexistent.txt
```
