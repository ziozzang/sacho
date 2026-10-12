.PHONY: build test check editor
build:
	cargo build --locked --release
check:
	cargo fmt --all -- --check
	cargo clippy --locked --all-targets -- -D warnings
test:
	cargo test --locked --all-targets
	python3 tests/smoke.py
editor:
	cd web && npm ci && npm run build:lua-editor
