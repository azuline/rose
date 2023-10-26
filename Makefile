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
	black --check .
	ruff .

lint:
	black .
	ruff --fix .

.PHONY: check test typecheck lintcheck lint
