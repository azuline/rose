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
	ruff .

lint:
	ruff format .
	ruff --fix .

.PHONY: check test typecheck lintcheck lint
