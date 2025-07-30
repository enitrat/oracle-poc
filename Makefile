.PHONY: help build run test dashboard dashboard-v2 dev clean

help:
	@echo "Available commands:"
	@echo "  make build       - Build the oracle and dashboard binaries"
	@echo "  make run         - Run the oracle services"
	@echo "  make dashboard   - Run the dashboard"
	@echo "  make dev         - Run both oracle and dashboard in separate terminals (requires tmux)"
	@echo "  make test        - Run all tests"
	@echo "  make clean       - Clean build artifacts"
	@echo "  make stop        - Stop the development environment"

build:
	cargo build --release

run:
	cargo run -- run

dashboard:
	cargo run --bin dashboard

dev:
	@if command -v tmux >/dev/null 2>&1; then \
		tmux new-session -d -s zamaoracle 'anvil --hardfork prague > /dev/null 2>&1 & docker-compose up -d && bun run script/deploy-contract.ts && RUST_LOG=info cargo run --bin zamaoracle'; \
		tmux split-window -h 'sleep 2 && cargo run --bin dashboard'; \
		tmux attach-session -t zamaoracle; \
	else \
		echo "tmux not found. Please install tmux or run the following commands in separate terminals:"; \
		echo "1. anvil"; \
		echo "2. docker-compose up -d"; \
		echo "3. bun run script/deploy-contract.ts"; \
		echo "4. cargo run -- run"; \
		echo "5. cargo run --bin dashboard"; \
		exit 1; \
	fi

stop:
	@echo "Stopping ZamaOracle development environment..."
	@tmux kill-session -t zamaoracle 2>/dev/null || echo "No tmux session found"
	@docker-compose down 2>/dev/null || echo "Docker compose not running"
	@rm -f .deploy-complete
	@lsof -i :8545 | awk 'NR>1 {print $$2}' | xargs kill -9
	@echo "Development environment stopped"

test:
	cargo test
	npm test

clean:
	cargo clean
	rm -rf target/
