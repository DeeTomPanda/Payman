BEGIN;
ALTER TABLE idempotency_keys DROP CONSTRAINT idempotency_keys_key_key;
ALTER TABLE idempotency_keys ADD CONSTRAINT idempotency_keys_key_business_id_key UNIQUE (key, business_id); -- link it with business id
ALTER TABLE idempotency_keys ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
ALTER TABLE idempotency_keys ALTER COLUMN response_status DROP NOT NULL;
COMMIT;