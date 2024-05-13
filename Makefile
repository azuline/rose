check: typecheck test lintcheck

# Build the Zig library for development.
build-zig:
	cd rose-zig && zig build -Doptimize=Debug

typecheck: build-zig
	mypy .

test-py: build-zig
	pytest -n logical .
	coverage html

test-zig:
	cd rose-zig && zig build test --summary all

test: test-zig test-py 

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
	cd rose-zig && zon2nix > deps.nix

.PHONY: check build-zig test-py test-zig test typecheck lintcheck lint clean nixify-zig-deps
