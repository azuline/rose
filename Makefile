check: typecheck test lintcheck

# Build the Zig library for development.
build-zig:
	cd rose-zig && zig build -Doptimize=Debug

typecheck:
	mypy .

test: build-zig
	pytest -n logical .
	coverage html

test-seq: build-zig
	pytest .
	coverage html

snapshot: build-zig
	pytest --snapshot-update .

lintcheck:
	ruff format --check .
	ruff check .
	prettier --check .

lint:
	ruff format .
	ruff check --fix .
	prettier --write .

clean:
	git clean -xdf

nixify-zig-deps:
	cd rose-zig && nix run 'github:Cloudef/zig2nix#zon2nix' -- build.zig.zon > deps.nix

.PHONY: build-zig check test typecheck lintcheck lint clean nixify-zig-deps
