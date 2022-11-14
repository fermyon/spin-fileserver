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

### Building from source and using

Prerequisites:

- [Rust](https://www.rust-lang.org/) at
  [1.56+](https://www.rust-lang.org/tools/install) with the `wasm32-wasi` target
  configured
- [Spin v0.1](https://github.com/fermyon/spin)

Compiling the component:

```shell
$ cargo build --release
```

### Testing

Prerequisites:

- [Rust](https://www.rust-lang.org/) at
  [1.56+](https://www.rust-lang.org/tools/install) with the `wasm32-wasi` target
  configured

Running test cases:

```shell
$ make test
```
