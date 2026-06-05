BEGIN;

CREATE TABLE invoice_line_items (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id          UUID NOT NULL REFERENCES invoices(id),
    description         TEXT NOT NULL,
    quantity            INTEGER NOT NULL CHECK (quantity > 0),
    unit_amount_cents   BIGINT NOT NULL CHECK (unit_amount_cents > 0),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_line_items_invoice_id ON invoice_line_items(invoice_id);

COMMIT;