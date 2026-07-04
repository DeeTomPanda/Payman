#!/bin/bash

for ((i=0; i<=100; i++)); do
  response=$(curl -s -o /dev/null -w "%{http_code}" -X POST http://localhost:8080/businesses \
    -H "Content-Type: application/json" \
    -d '{"name": "Corp Card Declined"}')

  echo "Request $i → HTTP $response"
done