# Static file server for Spin applications

A simple static file server as a [Spin](https://github.com/fermyon/spin) HTTP
component, written in Rust.

This component is now fully [componentized](https://component-model.bytecodealliance.org/) and
can be used with any runtime that supports `wasi:http@0.2.0-rc-2023-10-18`, such as
[Spin 2.0](https://developer.fermyon.com/spin/install), [wasmtime](https://github.com/bytecodealliance/wasmtime)
and [NGINX Unit](https://unit.nginx.org/).

- [Building from source](#building-from-source)
- [Testing](#testing)
- [Using the component](#using-the-component-as-part-of-a-spin-application)
  - [Running the file server](#running-the-file-server)
  - [Composing with the file server](#component-composition-with-the-file-server)
- [Configuration options](#configuration-options)

## Building from source

Prerequisites:

- [Rust](https://www.rust-lang.org/) at [1.72+](https://www.rust-lang.org/tools/install) with the `wasm32-wasi` target configured
- [cargo-component](https://github.com/bytecodealliance/cargo-component)
- [Spin v2.0](https://github.com/fermyon/spin) to run the component/examples

Compiling the component:

```shell
$ cargo component build --release
```

See the [examples](./examples) directory for examples of using and composing `spin-fileserver` with applications.

## Testing

Prerequisites:

- [Rust](https://www.rust-lang.org/) at
  [1.72+](https://www.rust-lang.org/tools/install) with the `wasm32-wasi` target
  configured

Running test cases:

```shell
$ make test
```

## Using the component as part of a Spin application

The easiest way to use this the Spin fileserver component in your application
is to add it via its Spin template.

To create a new Spin app based on this component, run:

```shell
$ spin new -t static-fileserver
```

To add this component to your existing Spin app, run:

```shell
$ spin add -t static-fileserver
```

If you're looking to upgrade the version of this component in your application from one
of the [releases](https://github.com/fermyon/spin-fileserver/releases), select the release
and corresponding checksum and update the component's `source` in the application's `spin.toml`, e.g.:

```toml
source = { url = "https://github.com/fermyon/spin-fileserver/releases/download/v0.1.0/spin_static_fs.wasm", digest = "sha256:96c76d9af86420b39eb6cd7be5550e3cb5d4cc4de572ce0fd1f6a29471536cb4" }
```

Next, we'll look at running the file server directly as well as using component
composition to integrate this component in with your application logic.

### Running the file server

Let's have a look at the component definition (from [spin.toml](./spin.toml)):

```toml
[[trigger.http]]
route = "/..."
component = "fs"

# For more on configuring a component, see: https://developer.fermyon.com/spin/writing-apps
[component.fs]
source = "target/wasm32-wasi/release/spin_static_fs.wasm"
files = [{ source = "", destination = "/" }]
[component.fs.build]
command = "make"
```

This component will recursively mount all files from the current directory and
will serve them at the configured route. If an `index.html` file is in the source root,
it will be served if no file is specified.

Running the static server:

```shell
$ spin up --listen 127.0.0.1:3000 --file spin.toml
```

At this point, the component is going to serve all files in the current
directory, and this can be tested using `curl`:

```shell
$ curl localhost:3000/LICENSE
                              Apache License
                        Version 2.0, January 2004
                    http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION
...
```

See also the [rust-standalone example](./examples/rust-standalone/) showing use of the file server
alongside a simple Rust-based application.

### Component composition with the file server

The file server can also be composed with application logic to form one binary that can be run
as a Spin application. See the following examples using the language and toolchains of your choice:

- [Rust](./examples/rust)
- [Javascript](./examples/javascript)
- [Python](./examples/python)

## Configuration options

The Spin fileserver supports various configuration options.

### Setting the cache header

Currently, this file server has a single cache header that it can set through
the `CACHE_CONTROL` environment variable. If no value is set, the default
`max-age=60` is used instead for all media types.

### Setting the fallback path

You can configure a `FALLBACK_PATH` environment variable that points to a file that
will be returned instead of the default 404 Not Found response. If no environment
value is set, the default behavior is to return a 404 Not Found response. This behavior
is useful for Single Page Applications that use view routers on the front-end like React and Vue.

```toml
# For more on configuring a component, see: https://developer.fermyon.com/spin/writing-apps#adding-environment-variables-to-components
[component.fs]
source = "target/wasm32-wasi/release/spin_static_fs.wasm"
files = [{ source = "test", destination = "/" }]
environment = { FALLBACK_PATH = "index.html" }
```

### Using a custom 404 document

You can configure a `CUSTOM_404_PATH` environment variable and point to a file that will be served instead of returning a plain 404 Not Found response. Consider the following sample where the `spin-fileserver` component is configured to serve all files from the `test` folder. The desired page must exist in the `test` folder to send a custom 404 HTML page (here, `404.html`) instead of a plain 404 Not Found response.

```toml
# For more on configuring a component, see: https://developer.fermyon.com/spin/writing-apps#adding-environment-variables-to-components
[component.fs]
source = "target/wasm32-wasi/release/spin_static_fs.wasm"
files = [{ source = "test", destination = "/" }]
environment = { CUSTOM_404_PATH = "404.html" }
```

### Fallback favicon

If you haven't specified a favicon in your HTML document, `spin-fileserver` will serve the [Spin logo](./spin-favicon.png) as the fallback favicon. The `spin-fileserver` also serves the fallback favicon if the file (called `favicon.ico` or `favicon.png`) specified in your `<link rel="shortcut icon" ...>` element does not exist.

Remember that there are situations where `spin-fileserver` cannot serve the fallback favicon if no `<link rel="shortcut icon" ...>` element is specified. Browsers try to find the favicon in the root directory of the origin (`somedomain.com/favicon.ico`). If the application doesn't listen for requests targeting that route, it can't intercept requests to non-existing favicons.
