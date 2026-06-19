# spin-fileserver-example (JavaScript)

This is an example of using [component
composition](https://component-model.bytecodealliance.org/creating-and-consuming/composing.html)
to compose an example app with
[spin-fileserver](https://github.com/fermyon/spin-fileserver), a reusable
component for serving static files.

## Prerequisites

- [Spin v2.0+](https://developer.fermyon.com/spin/install)
- [Rust](https://rustup.rs/)
- [cargo-component](https://github.com/bytecodealliance/cargo-component)
- [wac](https://github.com/bytecodealliance/wac)
- [NodeJS](https://nodejs.org/en/download)
- [componentize-js](https://github.com/dicej/componentize-js)
- [curl](https://curl.se/download.html) or a web browser for testing
  
Once you have Rust and NodeJS installed, the following should give you everything else:

```shell
rustup target add wasm32-wasip2
cargo install cargo-component
cargo install wac-cli
npm install -g @bytecodealliance/jco @bytecodealliance/componentize-js
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
