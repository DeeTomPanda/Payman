# AI_USAGE.md

## Tools used

**Github Co-pilot** and 
**Claude (claude.ai)** — used throughout the project for the following:

- Initial DB schema draft. I described the domain (businesses, customers, invoices, line items, payment attempts, webhooks) and asked for a Postgres schema. The output was roughly correct but I revised it significantly — details below.
- Axum handler boilerplate (routing setup, extractor syntax, `sqlx::query_as!` patterns). I know the domain logic; I don't have Axum's extractor API memorized.
- Explaining Postgres locking options (advisory locks, `FOR UPDATE`, serializable isolation) when I was deciding which mechanism to use for concurrent payments.
- Writing DESIGN.md, API.md, and this file — based on my own implementation and decisions, with Claude drafting the prose after I explained my reasoning.

---

## Three decisions I made myself, against or independent of AI suggestions

*1. Adding the `processing` state to the invoice state machine**

AI proposed a simple state machine: `open → paid` or `open → failed`.
I added `processing` as an intermediate state between `open` and `paid`.

Reason: without it, two concurrent `/pay` requests both read `open`,
both pass the state check, both call the PSP — potential double charge.
`processing` acts as an application-level lock on top of `FOR UPDATE`.
A second request wakes up from the DB lock, reads `processing`, fails
the state check, and exits cleanly without touching the PSP.
Two layers of concurrency protection instead of one.

---

**2. HTTP 202 response for PSP timeout/network error**

AI suggested returning 200 with a pending attempt on timeout.
I returned 202 Accepted instead.

Reason: 200 implies the request completed successfully. It didn't.
202 semantically means "received and being processed — outcome unknown."
This is honest — the PSP may or may not have charged the card.
The caller knows to poll `GET /invoices/:id` rather than assuming
success or failure. Lying with 200 could cause the client to
stop retrying when they should keep checking.

---

**3. Webhook retry budget with hard failure after 5 attempts**

AI's initial webhook worker retried indefinitely.
I added a hard limit of 5 attempts with exponential backoff,
after which the delivery is marked `failed` permanently.

Reason: infinite retries are operationally dangerous. A permanently
down receiver would fill the webhook_deliveries table indefinitely,
slow the worker, and waste resources. Marking failed after 5 attempts
forces the business to fix their endpoint and provides a clean
audit trail of what was attempted. Businesses can reconcile
missed events by polling `GET /invoices` directly.

---

## One thing AI got wrong that I had to correct


Claude's initial payment handler called the PSP first, then
inserted the payment_attempt record. I reversed this order —
insert the attempt as `pending` before calling the PSP.

Reason: if the service crashes after a successful PSP charge
but before the insert, the customer is charged with no record
in our system. Writing `pending` first means the worst case
is a pending record with no corresponding PSP call — safe to
retry. The audit trail is never missing a charge.


---

## What I actually built vs what AI generated

For decisions where AI generated the initial code, I verified
correctness by:
- Reading the generated SQL and checking it against the migration schema
- Tracing the transaction boundaries manually for the payment handler
- Running all 4 tests against a live Postgres instance
- Manually curling every endpoint after docker compose up