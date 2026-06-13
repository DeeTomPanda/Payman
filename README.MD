# Payman — Invoice & Payment Service

A minimal billing API built with Rust (Axum), PostgreSQL, and a mock PSP.


## Stack
- Rust + Axum
- PostgreSQL + sqlx
- Mock PSP (separate Rust binary)
- Docker Compose

## Running

```bash
docker compose build --no-cache
docker compose up
```

 App runs on port 8080, mock PSP on port 9090.

## curl Examples

### 1. Create a business (get your API key)
```bash
curl -X POST http://localhost:8080/businesses \
  -H "Content-Type: application/json" \
  -d '{"name": "Acme Corp"}'
```
Save the `api_key` from the response — it is shown once and never stored.

### 2. Create a customer
```bash
curl -X POST http://localhost:8080/customers \
  -H "Authorization: Bearer sk_live_xxx" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "John Doe",
    "email": "john@example.com"
  }'
```

### 3. Create an invoice
```bash
curl -X POST http://localhost:8080/invoices \
  -H "Authorization: Bearer sk_live_xxx" \
  -H "Content-Type: application/json" \
  -d '{
    "customer_id": "<customer-uuid>",
    "due_date": "2026-12-01",
    "line_items": [
      {
        "description": "Consulting",
        "quantity": 2,
        "unit_amount_cents": 5000
      }
    ]
  }'
```

### 4. Finalize invoice (draft → open)
```bash
curl -X POST http://localhost:8080/invoices/<invoice-uuid>/finalize \
  -H "Authorization: Bearer sk_live_xxx"
```

### 5. Attempt payment — success
```bash
curl -X POST http://localhost:8080/invoices/<invoice-uuid>/pay \
  -H "Authorization: Bearer sk_live_xxx" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: unique-key-001" \
  -d '{"card_token": "tok_success"}'
```

### 6. Attempt payment — failure
```bash
curl -X POST http://localhost:8080/invoices/<invoice-uuid>/pay \
  -H "Authorization: Bearer sk_live_xxx" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: unique-key-002" \
  -d '{"card_token": "tok_card_declined"}'
```

### 7. Register a webhook endpoint
```bash
curl -X POST http://localhost:8080/webhooks \
  -H "Authorization: Bearer sk_live_xxx" \
  -H "Content-Type: application/json" \
  -d '{"url": "https://your-server.com/webhook"}'
```

### Running Tests

```bash
make test-all                  # run everything
make test-success              # tok_success happy path
make test-decline              # tok_card_declined
make test-insufficient-funds   # tok_insufficient_funds
make test-timeout              # tok_timeout — invoice reverts to open
make test-network-error        # tok_network_error — invoice reverts to open
make test-concurrent           # N concurrent pay requests, only one succeeds
make test-ident-idempotency    # same key twice, same response
```

Also available:
```bash
make db-up       # start postgres
make db-reset    # full wipe and remigrate
make db-prepare  # regenerate .sqlx offline cache
```

## Mock PSP Tokens
| Token | Behavior |
|-------|---------|
| `tok_success` | Succeeds after 100ms |
| `tok_insufficient_funds` | Fails after 100ms |
| `tok_card_declined` | Fails after 100ms |
| `tok_timeout` | Sleeps 30s (service times out at 5s) |
| `tok_network_error` | Returns 500 |