BEGIN;
ALTER TABLE idempotency_keys ADD COLUMN invoice_id UUID NOT NULL REFERENCES invoices(id);
ALTER TABLE idempotency_keys DROP CONSTRAINT idempotency_keys_key_business_id_key;
CREATE UNIQUE INDEX ON idempotency_keys (key, business_id, invoice_id);
COMMIT;