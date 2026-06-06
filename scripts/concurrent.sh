#!/bin/bash

# Ensure clean slate for temporary files
rm -f /tmp/pay_result_*.json

CONCURRENT_REQUESTS=${1:-5}

# 1. Setup - Create business, customer, invoice
BUSINESS=$(curl -s -X POST http://localhost:8080/businesses \
  -H "Content-Type: application/json" \
  -d '{"name": "Corp Concurrent"}')
API_KEY=$(echo $BUSINESS | jq -r '.api_key')
BUSINESS_ID=$(echo $BUSINESS | jq -r '.business_id')

echo "API Key: $API_KEY"
echo "Business Created: $BUSINESS_ID"

CUSTOMER=$(curl -s -X POST http://localhost:8080/customers \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"name": "Test", "email": "test@example.com"}')
CUSTOMER_ID=$(echo $CUSTOMER | jq -r '.id')

echo "Customer Created: $CUSTOMER_ID"

INVOICE=$(curl -s -X POST http://localhost:8080/invoices \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d "{\"customer_id\": \"$CUSTOMER_ID\", \"due_date\": \"2026-12-01\", \"line_items\": [{\"description\": \"test\", \"quantity\": 1, \"unit_amount_cents\": 1000}]}")

INVOICE_ID=$(echo $INVOICE | jq -r '.invoice.id // .id')

echo "Invoice Created: $INVOICE_ID"

# 2. Finalize Invoice (Move from Draft to Open)
curl -s -X POST http://localhost:8080/invoices/$INVOICE_ID/finalize \
  -H "Authorization: Bearer $API_KEY" > /dev/null

echo "Firing $CONCURRENT_REQUESTS concurrent payment requests..."

# 3. Fire 5 concurrent pay requests with DIFFERENT idempotency keys
for ((i=1; i<=CONCURRENT_REQUESTS; i++)); do
  curl -s -X POST http://localhost:8080/payments/$INVOICE_ID/pay \
    -H "Authorization: Bearer $API_KEY" \
    -H "Content-Type: application/json" \
    -H "Idempotency-Key: concurrent-test-$i" \
    -d '{"card_token": "tok_success"}' \
    > /tmp/pay_result_$i.json &
done

# Wait for all background curl processes to finish
wait

echo "Analyzing transaction outcomes..."

# 4. Count how many succeeded
SUCCESS_COUNT=0
for ((i=1; i<=CONCURRENT_REQUESTS; i++)); do
  # Debug log so you can see exactly what each thread returned in your demo
  echo "Response $i: $(cat /tmp/pay_result_$i.json)"
  
  STATUS=$(cat /tmp/pay_result_$i.json | jq -r '.status // empty')
  if [ "$STATUS" = "succeeded" ]; then
    SUCCESS_COUNT=$((SUCCESS_COUNT + 1))
  fi
done

# 5. Assert exactly one succeeded
if [ "$SUCCESS_COUNT" -eq 1 ]; then
  echo "PASS: Exactly 1 out of $CONCURRENT_REQUESTS concurrent payments succeeded."
else
  echo "FAIL: $SUCCESS_COUNT payments succeeded, expected exactly 1."
  exit 1
fi

# 6. Assert invoice status is updated to paid
FINAL_STATE=$(curl -s http://localhost:8080/invoices/$INVOICE_ID \
  -H "Authorization: Bearer $API_KEY" | jq -r '.invoice.state // .state')

if [ "$FINAL_STATE" = "paid" ]; then
  echo "PASS: Invoice final state is 'paid'."
else
  echo "FAIL: Invoice state is '$FINAL_STATE', expected 'paid'."
  exit 1
fi