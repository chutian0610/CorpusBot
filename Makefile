.PHONY: install dev-local dev-tauri fmt fmt-check lint test test-e2e test-acceptance typecheck build check clean

dev-local:
	@cargo run --bin corpusbot-server & server_pid=$$!; \
	trap 'kill $$server_pid 2>/dev/null' EXIT; \
	pnpm dev:web:local; status=$$?; \
	exit $$status

dev-tauri:
	@pnpm exec tauri dev

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

test-acceptance:
	scripts/mvp-acceptance.sh

typecheck:
	pnpm typecheck

build:
	pnpm build

check: fmt-check lint test typecheck build test-e2e test-acceptance

clean:
	cargo clean
