BEGIN;
DROP INDEX IF EXISTS idx_invoices_business_id;
DROP INDEX IF EXISTS idx_invoices_state;

CREATE INDEX idx_invoices_business_id ON invoices(business_id);
CREATE INDEX idx_invoices_business_state ON invoices (business_id, state);

COMMIT;