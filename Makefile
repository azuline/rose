check: typecheck test lintcheck

# Build the Zig library for development.
build-zig:
	cd rose_zig && zig build -Doptimize=Debug

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

.PHONY: build-zig check test typecheck lintcheck lint clean
