.PHONY: fmt test lint python rust build sdist check clean

PYTHON_VENV ?= /tmp/restflow-python-venv
CARGO_TARGET_DIR ?= /tmp/restflow-python-target
WHEEL_DIR ?= /tmp/restflow-python-dist

export PYTHON_VENV
export CARGO_TARGET_DIR
export WHEEL_DIR

fmt:
	cargo fmt --all

test:
	cargo test
	PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s python/tests

lint:
	cargo fmt --all --check
	cargo clippy --all-targets -- -D warnings

python:
	scripts/check.sh ci

rust:
	cargo test
	cargo clippy --all-targets -- -D warnings

build:
	scripts/check.sh build

sdist:
	scripts/check.sh sdist

check: lint test python

clean:
	rm -rf "$(CARGO_TARGET_DIR)" "$(PYTHON_VENV)" "$(WHEEL_DIR)"
