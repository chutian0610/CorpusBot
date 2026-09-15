.PHONY: install fmt fmt-check lint test test-e2e typecheck build check clean

install:
	pnpm install

fmt:
	cargo fmt --all
	pnpm exec prettier --write .

fmt-check:
	cargo fmt --all --check
	pnpm exec prettier --check .

lint:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace
	pnpm test

test-e2e:
	pnpm exec playwright test

typecheck:
	pnpm typecheck

build:
	pnpm build

check: fmt-check lint test typecheck build test-e2e

clean:
	cargo clean
