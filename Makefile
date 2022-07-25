.PHONY: default
default: test
	cargo build --release

.PHONY: test
test: lint test-unit

.PHONY: lint
lint:
	cargo clippy --all-features -- -D warnings
	cargo fmt -- --check

.PHONY: test-unit
test-unit:
	RUST_LOG=$(LOG_LEVEL) cargo test --target=$$(rustc -vV | sed -n 's|host: ||p')
