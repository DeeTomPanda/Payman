# 1. Create business
business_response=$(curl -s -X POST http://localhost:8080/businesses \
  -H "Content-Type: application/json" \
  -d '{"name": "Corp Insufficient Funds"}')

echo "$business_response" | jq .

# save the api_key from response
export API_KEY=$(echo "$business_response" | jq -r '.api_key')

# 2. Create customer
export CUSTOMER_ID=$(curl -s -X POST http://localhost:8080/customers \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"name": "Jane Doe", "email": "jane@example.com"}' | jq -r '.id')

echo "Customer: $CUSTOMER_ID"

# 3. Create invoice
response=$(curl -s -X POST http://localhost:8080/invoices \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d "{
    \"customer_id\": \"$CUSTOMER_ID\",
    \"due_date\": \"2026-12-01\",
    \"line_items\": [
      {\"description\": \"Consulting\", \"quantity\": 2, \"unit_amount_cents\": 5000}
    ]
  }")

echo "$response" | jq .

export INVOICE_ID=$(echo "$response" | jq -r '.invoice.id')
export INVOICE_VERSION=$(echo "$response" | jq -r '.invoice.versioning')

echo "Invoice: $INVOICE_ID"
echo "Versioning: $INVOICE_VERSION"

# 4. Finalize invoice (draft → open)
curl -s -X POST http://localhost:8080/invoices/$INVOICE_ID/finalize \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"versioning": 1}' | jq .

# 5. Simulate 
curl -s -X POST http://localhost:8080/payments/$INVOICE_ID/pay \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: pay-001" \
  -d '{
        "card_token": "tok_insufficent_funds",
        "versioning":2
      }' | jq .



