BEGIN;

CREATE TYPE payment_status AS ENUM (
    'pending',
    'succeeded',
    'failed'
);

CREATE TABLE payment_attempts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    invoice_id      UUID NOT NULL REFERENCES invoices(id),
    status          payment_status NOT NULL DEFAULT 'pending',
    card_token      TEXT NOT NULL,
    psp_reference   TEXT,
    failure_code    TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_payment_attempts_invoice_id ON payment_attempts(invoice_id);

COMMIT;