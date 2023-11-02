# Rust fileserver example (standalone)

This is an example of using [spin-fileserver](https://github.com/fermyon/spin-fileserver)
as a separate component in a Spin application. In other words, this example demonstrates
standalone use of this component as opposed to the composition approaches demonstrated
in the [rust](../rust), [javascript](../javascript/) and [pythong](../python/) examples.

## Prerequisites

- [Spin v2.0+](https://developer.fermyon.com/spin/install)
- [Rust](https://rustup.rs/), including the `wasm32-wasi` target
- [curl](https://curl.se/download.html) or a web browser for testing
  
Once you have Rust installed, the following should give you everything else:

```shell
rustup target add wasm32-wasi
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
