# Static file server for Spin applications

A simple static file server as a [Spin](https://github.com/fermyon/spin) HTTP
component, written in Rust.

### Using the component as part of a Spin application

Let's have a look at the component definition (from `spin.toml`):

```toml
[[component]]
source = "spin_static_fs.wasm"
id = "fileserver"
files = [{ source = "", destination = "/" }]
[component.trigger]
route = "/..."
```

This component will recursively mount all files from the current directory and
will serve them. If an `index.html` file is in the source root, it will be served if no file is specified.

Running the static server:

```shell
$ spin up --listen 127.0.0.1:3000 --file spin.toml
```

At this point, the component is going to serve all files in the current
directory, and this can be tested using `curl`:

```
$ curl localhost:3000/LICENSE
                              Apache License
                        Version 2.0, January 2004
                    http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION
...
```

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
[[component]]
source = "target/wasm32-wasi/release/spin_static_fs.wasm"
id = "fs"
files = [{ source = "test", destination = "/" }]
environment = { FALLBACK_PATH = "index.html" }
```

### Using a custom 404 document

You can configure a `CUSTOM_404_PATH` environment variable and point to a file that will be served instead of returning a plain 404 Not Found response. Consider the following sample where the `spin-fileserver` component is configured to serve all files from the `test` folder. The desired page must exist in the `test` folder to send a custom 404 HTML page (here, `404.html`) instead of a plain 404 Not Found response.

```toml
# For more on configuring a component, see: https://developer.fermyon.com/spin/writing-apps#adding-environment-variables-to-components
[[component]]
source = "target/wasm32-wasi/release/spin_static_fs.wasm"
id = "fs"
files = [{ source = "test", destination = "/" }]
environment = { CUSTOM_404_PATH = "404.html" }
```

### Fallback favicon

If you haven't specified a favicon in your HTML document, `spin-fileserver` will serve the [Spin logo](./spin-favicon.png) as the fallback favicon. The `spin-fileserver` also serves the fallback favicon if the file (called `favicon.ico` or `favicon.png`) specified in your `<link rel="shortcut icon" ...>` element does not exist.

Remember that there are situations where `spin-fileserver` cannot serve the fallback favicon if no `<link rel="shortcut icon" ...>` element is specified. Browsers try to find the favicon in the root directory of the origin (`somedomain.com/favicon.ico`). If the application doesn't listen for requests targeting that route, it can't intercept requests to non-existing favicons.

### Building from source and using

Prerequisites:

- [Rust](https://www.rust-lang.org/) at [1.72+](https://www.rust-lang.org/tools/install) with the `wasm32-wasi` target configured
- [cargo-component](https://github.com/bytecodealliance/cargo-component)
- [Spin v2.0](https://github.com/fermyon/spin)

Compiling the component:

```shell
$ cargo component build --release
```

See the [examples](./examples) directory for examples of composing `spin-fileserver` with applications.

### Testing

Prerequisites:

- [Rust](https://www.rust-lang.org/) at
  [1.72+](https://www.rust-lang.org/tools/install) with the `wasm32-wasi` target
  configured

Running test cases:

```shell
$ make test
```
