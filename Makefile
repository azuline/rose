check: typecheck test lintcheck

typecheck:
	mypy .

test:
	pytest -n logical --cov=. .
	coverage html

test-seq:
	pytest --cov=. .
	coverage html

lintcheck:
	black --check .
	ruff .

lint:
	black .
	ruff --fix .

.PHONY: check test typecheck lintcheck lint
