check: typecheck test lintcheck

typecheck:
	mypy .

test:
	pytest --cov=. .
	coverage html

lintcheck:
	black --check .
	ruff .

lint:
	black .
	ruff --fix .

.PHONY: check test typecheck lintcheck lint
