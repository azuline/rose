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
	prettier --check .

lint:
	ruff format .
	ruff --fix .
	prettier --write .

.PHONY: check test typecheck lintcheck lint
