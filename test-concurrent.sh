#!/bin/bash
# Test concurrent funding - does optimistic locking prevent lost updates?

WALLET_ID="89d61ff6-86c6-4246-8de1-e897a5d94bc9"

echo "Current balance:"
curl -s http://localhost:3000/wallets/$WALLET_ID | jq '.data.balance'

echo ""
echo "Launching 10 concurrent $10 funding requests..."

# Launch 10 requests in parallel
for i in {1..10}; do
  curl -s -X POST http://localhost:3000/wallets/$WALLET_ID/fund \
    -H "Content-Type: application/json" \
    -d '{"amount": "10.00"}' &
done

# Wait for all to complete
wait

echo ""
echo "Final balance (should be exactly $170 if all succeeded):"
curl -s http://localhost:3000/wallets/$WALLET_ID | jq '.data.balance'
