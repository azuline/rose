check: typecheck test lintcheck

typecheck:
	mypy .

test:
	pytest -n logical .
	coverage html

test-seq:
	pytest .
	coverage html

lintcheck:
	ruff format --check .
	ruff check .
	prettier --check .

lint:
	ruff format .
	ruff check --fix .
	prettier --write .

.PHONY: check test typecheck lintcheck lint
