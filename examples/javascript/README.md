# spin-fileserver-example (JavaScript)

This is an example of using [component
composition](https://component-model.bytecodealliance.org/creating-and-consuming/composing.html)
to compose an example app with
[spin-fileserver](https://github.com/fermyon/spin-fileserver), a reusable
component for serving static files.

## Prerequisites

- [Rust](https://rustup.rs/)
- [cargo-component](https://github.com/bytecodealliance/cargo-component)
- [wasm-tools](https://github.com/bytecodealliance/wasm-tools/)
  - Note that you'll need [this fork](https://github.com/dicej/wasm-tools/tree/wasm-compose-resource-imports) until [this PR](https://github.com/bytecodealliance/wasm-tools/pull/1261) has been merged and released.
- [NodeJS](https://nodejs.org/en/download)
- [componentize-js](https://github.com/dicej/componentize-js)
- [curl](https://curl.se/download.html) or a web browser for testing
  
Once you have Rust and NodeJS installed, the following should give you everything else:

*NOTE*: Until https://github.com/bytecodealliance/componentize-js/pull/69 has
been merged, you'll need to build and install `componentize-js` from source
using
https://github.com/dicej/componentize-js/tree/imported-resource-destructors
instead of the `npm install` command below.  See the README.md in that
repository for instructions.

```shell
rustup target add wasm32-wasi
cargo install cargo-component
cargo install --locked --git https://github.com/dicej/wasm-tools \
    --branch wasm-compose-resource-imports wasm-tools
# See NOTE above for installing `componentize-js`
# npm install @bytecodealliance/componentize-js
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
