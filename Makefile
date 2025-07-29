.PHONY: help build run test dashboard dev clean

help:
	@echo "Available commands:"
	@echo "  make build       - Build the oracle and dashboard binaries"
	@echo "  make run         - Run the oracle services"
	@echo "  make dashboard   - Run the live dashboard"
	@echo "  make dev         - Run both oracle and dashboard in separate terminals (requires tmux)"
	@echo "  make test        - Run all tests"
	@echo "  make clean       - Clean build artifacts"

build:
	cargo build --release

run:
	cargo run -- run

dashboard:
	cargo run --bin dashboard

dev:
	@if command -v tmux >/dev/null 2>&1; then \
		tmux new-session -d -s zamaoracle 'cargo run -- run'; \
		tmux split-window -h 'cargo run --bin dashboard'; \
		tmux attach-session -t zamaoracle; \
	else \
		echo "tmux not found. Please install tmux or run oracle and dashboard in separate terminals."; \
		exit 1; \
	fi

test:
	cargo test
	npm test

clean:
	cargo clean
	rm -rf target/