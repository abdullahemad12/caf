.PHONY: run watch check test clippy build

run-daemon:
	cargo run -- $(ARGS)

run-daemon-bootstrap:
	cargo run -- --no-bootstrap $(ARGS)

watch-daemon-bootstrap:
	watchexec --restart -e rs,toml -- cargo run -- --no-bootstrap $(ARGS)

check:
	cargo check

test:
	cargo test

clippy:
	cargo clippy

build:
	cargo build
