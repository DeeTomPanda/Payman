.PHONY: db-up db-down db-reset db-migrate db-prepare help test test-all test-decline test-ident-idempotency 
		test-insufficient-funds test-network-error test-success test-timeout test-concurrent

CONCURRENCY_LIMIT ?= 5

help:
	@echo "Available targets:"
	@echo "  db-up      - Start the database with docker compose"
	@echo "  db-down    - Stop and remove all volumes (clean slate)"
	@echo "  db-reset   - Full reset: down -v + up with migrations"
	@echo "  db-migrate - Run SQLx migrations"
	@echo "  db-prepare - Prepare .sqlx files"
	@echo "  test       - Run all payment script tests"
	@echo "  test-all   			  - Run all tests once"
	@echo "  test-decline             - Run the declined payment test script"
	@echo "  test-ident-idempotency   - Run the idempotency test script"
	@echo "  test-insufficient-funds - Run the insufficient funds test script"
	@echo "  test-network-error       - Run the network error test script"
	@echo "  test-success             - Run the successful payment test script"
	@echo "  test-timeout             - Run the timeout test script"
	@echo "  test-concurrent            - Run the timeout test script"

db-up:
	docker compose up -d postgres

db-down:
	docker compose down -v

db-migrate:
	sqlx migrate run

db-prepare:
	cargo sqlx prepare

db-reset: db-down db-up db-migrate db-prepare
	@echo "Database reset complete"

test: test-all

test-decline:
	@echo "Testing declined scenario"
	@./scripts/card_declined.sh
	@echo "\n"

test-ident-idempotency:
	@echo "Testing idempotency scenario"
	@./scripts/ident_idempotency.sh
	@echo "\n"

test-insufficient-funds:
	@echo "Testing insufficient funds scenario"
	@./scripts/insufficent_funds.sh
	@echo "\n"

test-network-error:
	@echo "Testing network error scenario"
	@./scripts/network_error.sh
	@echo "\n"

test-success:
	@echo "Testing success scenario"
	@./scripts/success.sh
	@echo "\n"

test-timeout:
	@echo "Testing timeout scenario"
	@./scripts/timeout.sh
	@echo "\n"

test-concurrent:
	@echo "Testing concurrent requests scenario"
	@./scripts/concurrent.sh ${CONCURRENCY_LIMIT}
	@echo "\n"

test-all:
	@$(MAKE) test-decline
	@sleep 5
	@$(MAKE) test-ident-idempotency
	@sleep 5
	@$(MAKE) test-insufficient-funds
	@sleep 5
	@$(MAKE) test-network-error
	@sleep 5
	@$(MAKE) test-success
	@sleep 5
	@$(MAKE) test-timeout
	@sleep 5
	@$(MAKE) test-concurrent
	@echo "All tests complete"


