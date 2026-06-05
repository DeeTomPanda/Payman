BEGIN;

CREATE TYPE delivery_status AS ENUM (
    'pending',
    'delivered',
    'failed'
);

CREATE TABLE webhook_deliveries (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    webhook_endpoint_id UUID NOT NULL REFERENCES webhook_endpoints(id),
    event_type          TEXT NOT NULL,
    payload             JSONB NOT NULL,
    status              delivery_status NOT NULL DEFAULT 'pending',
    attempt_count       INTEGER NOT NULL DEFAULT 0,
    next_retry_at       TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_webhook_deliveries_endpoint_id 
    ON webhook_deliveries(webhook_endpoint_id);
CREATE INDEX idx_webhook_deliveries_next_retry_at 
    ON webhook_deliveries(next_retry_at);
CREATE INDEX idx_webhook_deliveries_status 
    ON webhook_deliveries(status);

COMMIT;