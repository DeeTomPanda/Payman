# API Documentation

Base URL: `http://localhost:8080`

All protected endpoints require:
```
Authorization: Bearer sk_live_<key>
```

---

## Error Format

All errors return a consistent JSON body with a machine-readable `code` and a human-readable `message`:

```json
{
  "error": {
    "code": "invalid_state_transition",
    "message": "cannot pay an invoice in 'paid' state, must be 'open'"
  }
}
```

**`code` values**

| Code | Status | Meaning |
|------|--------|---------|
| `not_found` | `404` | Resource not found or not visible to your business |
| `unauthorized` | `401` | Missing or invalid API key |
| `bad_request` | `400` | Invalid input |
| `conflict` | `409` | Duplicate resource — e.g. email already exists |
| `invalid_state_transition` | `422` | Operation not valid for the invoice's current state |
| `database_error` | `500` | Internal database error |
| `internal_error` | `500` | Unexpected internal error |

`database_error` and `internal_error` do not expose internal details in `message` — both return `"internal server error"`.

---

## Businesses

### Create Business
`POST /businesses`

No authentication required. Creates a business and returns a one-time API key. **Save the key immediately — it is never shown again.**

**Request**
```json
{
  "name": "Medium Corp"
}
```

| Field | Type | Rules |
|-------|------|-------|
| `name` | string | 2–100 characters |

**Response `200`**
```json
{
  "business_id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
  "api_key": "sk_live_some_secret_la_la_la",
  "prefix": "sk_live_a1b2c3",
  "warning": "save this key now. It will never be shown again."
}
```

---

## Customers

All endpoints scoped to the authenticated business.

### Create Customer
`POST /customers`

**Request**
```json
{
  "name": "Jane Smith",
  "email": "jane@example.com"
}
```

| Field | Type | Rules |
|-------|------|-------|
| `name` | string | 2–100 characters |
| `email` | string | valid email, max 254 chars, unique per business |

**Response `200`**
```json
{
  "id": "a1b2c3d4-...",
  "business_id": "f47ac10b-...",
  "name": "Jane Smith",
  "email": "jane@example.com",
  "created_at": "2026-01-01T00:00:00Z"
}
```

**Errors**
- `409` — a customer with this email already exists for your business

---

### Get Customer
`GET /customers/:id`

**Response `200`** — same shape as Create Customer response.

**Errors**
- `404` — customer not found or belongs to a different business

---

### List Customers
`GET /customers`

Returns all customers for the authenticated business, ordered by `created_at` descending.

**Response `200`**
```json
[
  {
    "id": "a1b2c3d4-...",
    "business_id": "f47ac10b-...",
    "name": "Jane Smith",
    "email": "jane@example.com",
    "created_at": "2026-01-01T00:00:00Z"
  }
]
```

---

## Invoices

All endpoints scoped to the authenticated business.

Invoices are created in `draft` state. Call `POST /invoices/:id/finalize` to move them to `open` and make them payable.

### Invoice States

| State | Description |
|-------|-------------|
| `draft` | Created, not yet payable |
| `open` | Finalized, ready for payment |
| `processing` | Payment attempt in progress |
| `paid` | Successfully paid (terminal) |
| `void` | Cancelled (terminal) |
| `uncollectible` | Marked as uncollectible (terminal) |

---

### Create Invoice
`POST /invoices`

Server always computes `total_cents` from line items. Any client-supplied total is ignored.

**Request**
```json
{
  "customer_id": "a1b2c3d4-...",
  "due_date": "2026-12-31",
  "line_items": [
    {
      "description": "Consulting — January",
      "quantity": 2,
      "unit_amount_cents": 50000
    }
  ]
}
```

| Field | Type | Rules |
|-------|------|-------|
| `customer_id` | UUID | must belong to your business |
| `due_date` | date | `YYYY-MM-DD` |
| `line_items` | array | at least one required |
| `line_items[].description` | string | non-empty |
| `line_items[].quantity` | integer | > 0 |
| `line_items[].unit_amount_cents` | integer | > 0, in USD cents |

**Response `200`**
```json
{
  "invoice": {
    "id": "b2c3d4e5-...",
    "business_id": "f47ac10b-...",
    "customer_id": "a1b2c3d4-...",
    "state": "draft",
    "total_cents": 100000,
    "due_date": "2026-12-31",
    "created_at": "2026-01-01T00:00:00Z",
    "updated_at": "2026-01-01T00:00:00Z"
  },
  "line_items": [
    {
      "id": "c3d4e5f6-...",
      "invoice_id": "b2c3d4e5-...",
      "description": "Consulting — January",
      "quantity": 2,
      "unit_amount_cents": 50000,
      "created_at": "2026-01-01T00:00:00Z"
    }
  ]
}
```

**Errors**
- `400` — no line items, quantity ≤ 0, unit_amount_cents ≤ 0, or computed total ≤ 0
- `404` — customer not found or belongs to a different business

---

### Get Invoice
`GET /invoices/:id`

**Response `200`** — same shape as Create Invoice response (invoice + line_items).

**Errors**
- `404` — invoice not found or belongs to a different business

---

### List Invoices
`GET /invoices?state=<state>`

| Query param | Type | Required |
|-------------|------|----------|
| `state` | string | no — omit to return all states |

Valid `state` values: `draft`, `open`, `processing`, `paid`, `void`, `uncollectible`

Returns invoices ordered by `created_at` descending.

**Response `200`**
```json
[
  {
    "id": "b2c3d4e5-...",
    "business_id": "f47ac10b-...",
    "customer_id": "a1b2c3d4-...",
    "state": "open",
    "total_cents": 100000,
    "due_date": "2026-12-31",
    "created_at": "2026-01-01T00:00:00Z",
    "updated_at": "2026-01-01T00:00:00Z"
  }
]
```

---

### Finalize Invoice
`POST /invoices/:id/finalize`

Transitions invoice from `draft` → `open`. Fires an `invoice.created` webhook.

**Request body** — none

**Response `200`** — Invoice object (without line_items)

```json
{
  "id": "b2c3d4e5-...",
  "business_id": "f47ac10b-...",
  "customer_id": "a1b2c3d4-...",
  "state": "open",
  "total_cents": 100000,
  "due_date": "2026-12-31",
  "created_at": "2026-01-01T00:00:00Z",
  "updated_at": "2026-01-01T00:00:00Z"
}
```

**Errors**
- `404` — invoice not found
- `422` — invoice is not in `draft` state

---

### Void Invoice
`POST /invoices/:id/void`

Transitions invoice from `draft` → `void` or `open` → `void`. Terminal — cannot be undone.

**Request body** — none

**Response `200`** — Invoice object (without line_items)

**Errors**
- `404` — invoice not found
- `422` — invoice is in `processing`, `paid`, or `uncollectible` state

---

## Payments

### Pay Invoice
`POST /invoices/:id/pay`

Attempts payment for an invoice. Invoice must be in `open` state.

Requires an `Idempotency-Key` header. Reusing the same key within 24 hours returns the original response without making a second PSP call.

**Headers**

| Header | Required | Description |
|--------|----------|-------------|
| `Idempotency-Key` | yes | Unique string per payment attempt. Use a new key when retrying after a timeout. |

**Request**
```json
{
  "card_token": "tok_success"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `card_token` | string | Token from card tokenisation. See mock PSP tokens below. |

**Response `200` — succeeded or failed**
```json
{
  "id": "d4e5f6a7-...",
  "invoice_id": "b2c3d4e5-...",
  "status": "succeeded",
  "card_token": "tok_success",
  "psp_reference": "psp_abc123",
  "failure_code": null,
  "created_at": "2026-01-01T00:00:00Z",
  "updated_at": "2026-01-01T00:00:00Z"
}
```

**Response `202 Accepted` — PSP timed out or network error**

Same body shape, `status` will be `"pending"`. The invoice remains in `processing` state while the reconciliation worker attempts to determine the outcome. 
Do not retry immediately  the worker will resolve the attempt automatically.If the attempt expires after 24 hours without resolution, the invoice is reverted to `open` and you may retry with a new `Idempotency-Key`.

**`status` values**

| Value | Meaning |
|-------|---------|
| `pending` | PSP did not respond in time (202 response) |
| `succeeded` | Payment accepted by PSP |
| `failed` | PSP declined — check `failure_code` |

**`failure_code` values**

| Code | Meaning |
|------|---------|
| `insufficient_funds` | Card has insufficient funds |
| `card_declined` | Card was declined |
| `unknown` | Unrecognised PSP failure |

**Errors**
- `400` — missing `Idempotency-Key` header, or empty `card_token`
- `404` — invoice not found
- `422` — invoice is not in `open` state (already paid, void, processing, etc.)


## Payment Reconciliation

When a payment attempt times out or encounters a network error, the invoice is 
left in `processing` state and the attempt in `pending`. A background worker 
automatically reconciles these using exponential backoff.

### Retry Schedule

| Attempt | Delay |
|---------|-------|
| 1 | 1 minute |
| 2 | 2 minutes |
| 3 | 4 minutes |
| 4 | 8 minutes |
| 5+ | 16 minutes (capped) |

After 24 hours the attempt is marked `failed` with `failure_code: reconciliation_timeout` 
and the invoice is reverted to `open`.

### What the worker does

- Polls `GET /charge/:attempt_id` on the PSP
- `200 succeeded` → marks attempt `succeeded`, invoice `paid`, fires `invoice.paid` webhook
- `200 failed` → marks attempt `failed`, invoice reverted to `open`, fires `invoice.payment_failed` webhook  
- `404` → PSP still processing, schedules next retry per backoff table
- Timeout / network error → leaves as pending, retries next tick

---

## Webhook Endpoints

All endpoints scoped to the authenticated business.

### Register Endpoint
`POST /webhooks`

**Request**
```json
{
  "url": "https://your-server.com/webhooks"
}
```

| Field | Type | Rules |
|-------|------|-------|
| `url` | string | must start with `http://` or `https://` |

**Response `200`**
```json
{
  "id": "e5f6a7b8-...",
  "business_id": "f47ac10b-...",
  "url": "https://your-server.com/webhooks",
  "secret": "whsec_a1b2c3d4e5f6...",
  "active": true,
  "created_at": "2026-01-01T00:00:00Z"
}
```

The `secret` is used to verify webhook signatures. Store it securely.

**Errors**
- `400` — empty URL or URL doesn't start with http/https

---

### Get Endpoint
`GET /webhooks/:id`

**Response `200`** — same shape as Register Endpoint response.

**Errors**
- `404` — endpoint not found or belongs to a different business

---

### List Endpoints
`GET /webhooks`

**Response `200`**
```json
[
  {
    "id": "e5f6a7b8-...",
    "business_id": "f47ac10b-...",
    "url": "https://your-server.com/webhooks",
    "secret": "whsec_a1b2c3d4e5f6...",
    "active": true,
    "created_at": "2026-01-01T00:00:00Z"
  }
]
```

---

### Delete Endpoint
`DELETE /webhooks/:id`

Soft-deletes the endpoint (marks `active = false`). No further webhooks will be delivered to this URL.

**Response `200`**
```json
{
  "deleted": true
}
```

**Errors**
- `404` — endpoint not found or belongs to a different business

---

## Webhook Deliveries

Webhooks are delivered asynchronously — they do not block the API response.

### Events

| Event | Trigger |
|-------|---------|
| `invoice.created` | Invoice finalized (`POST /invoices/:id/finalize`) |
| `invoice.paid` | Payment succeeded |
| `invoice.payment_failed` | Payment declined by PSP |

### Payload Shape

```json
{
  "event": "invoice.paid",
  "invoice_id": "b2c3d4e5-...",
  "business_id": "f47ac10b-...",
  "timestamp": "2026-01-01T00:00:00Z"
}
```

### Retry Policy

| Attempt | Delay after previous failure |
|---------|------------------------------|
| 1 | immediate |
| 2 | 2 minutes |
| 3 | 4 minutes |
| 4 | 8 minutes |
| 5 (final) | 16 minutes |

Delay formula: `2^attempt_count minutes`. After 5 attempts the delivery is marked `failed` and will not be retried. Exhausted deliveries remain in the `webhook_deliveries` table and can be queried for reconciliation.

A delivery attempt times out after 5 seconds. A non-2xx response is treated as failure.

---

## Mock PSP Tokens

The following tokens can be used in `POST /invoices/:id/pay` during development:

| Token | Behaviour |
|-------|-----------|
| `tok_success` | Returns `succeeded` after ~100ms |
| `tok_insufficient_funds` | Returns `failed` with `insufficient_funds` after ~100ms |
| `tok_card_declined` | Returns `failed` with `card_declined` after ~100ms |
| `tok_timeout` | Sleeps 30s — your service times out after 5s and returns `202` |
| `tok_network_error` | Returns 500 or drops connection — your service returns `202` |

---

## Health Check

`GET /health`

No authentication required.

**Response `200`**
```
OK
```

# Mock PSP (External Payment Service)

Base URL: `http://localhost:9090`

This service simulates an external payment processor used by the Invoice service.

## POST /charge

Creates a simulated payment attempt.

### Request

````json
{
  "card_token": "tok_success | tok_insufficient_funds | tok_card_declined | tok_timeout | tok_network_error",
  "attempt_id": "uuid"
}
````

````markdown
### Response `200` — charge completed
```json
{
  "status": "succeeded | failed",
  "psp_ref": "uuid",
  "code": "insufficient_funds | card_declined | null"
}
```

### Response `500` — network error (tok_network_error)

### Response `400` — unknown card token

### Notes
- `tok_timeout` sleeps 30 seconds before responding — your service must time out before this
- `psp_ref` is present only when `status` is `succeeded`

## GET /charge/:attempt_id

Retrieves the outcome of a previously submitted charge. Used by the reconciliation worker to check the result of timed-out payment attempts.

### Response `200` — outcome available
```json
{
  "status": "succeeded",
  "psp_ref": "uuid",
  "code": null
}
```

### Response `404` — outcome not yet available
The charge is still being processed. Retry later.