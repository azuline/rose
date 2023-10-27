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
	ruff .
	ruff format --check .

lint:
	ruff --fix .
	ruff format .

.PHONY: check test typecheck lintcheck lint
