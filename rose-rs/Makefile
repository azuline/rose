check: typecheck test lintcheck

build:
	cargo build

typecheck:
	cargo check

test:
	cargo test

lintcheck:
	cargo fmt -- --check
	cargo clippy -- -D warnings

lint:
	cargo fmt
	cargo clippy --fix --allow-dirty --allow-staged

docs:
	cargo doc --no-deps --open

bench:
	cargo bench

clean:
	cargo clean

.PHONY: check build typecheck test lintcheck lint docs bench clean
