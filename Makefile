.PHONY: test
test: lint test-unit

.PHONY: lint 
lint:
	cargo clippy --all-targets --all-features -- -D warnings
	cargo fmt --all -- --check

.PHONY: test-unit
test-unit:
	RUST_LOG=$(LOG_LEVEL) cargo test --all --no-fail-fast --target=$$(rustc -vV | sed -n 's|host: ||p') -- --nocapture --include-ignored
