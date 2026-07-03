.PHONY: default
default: test
	cargo build --release --target wasm32-wasip2

.PHONY: test
test: lint test-unit

.PHONY: lint
lint:
	cargo clippy --all-features -- -D warnings
	cargo fmt -- --check

.PHONY: test-unit
test-unit:
	RUST_LOG=$(LOG_LEVEL) cargo test --target=$$(rustc -vV | sed -n 's|host: ||p')

.PHONY: spin-test
spin-test:
	RUST_LOG=$(LOG_LEVEL) spin test