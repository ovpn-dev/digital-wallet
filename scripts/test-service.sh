#!/bin/bash
# Quick test script to verify wallet service is working

set -e  # Exit on any error

echo "🧪 Testing Wallet Service..."
echo ""

BASE_URL="http://localhost:3000"

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Test 1: Health check
echo "Test 1: Health Check"
response=$(curl -s $BASE_URL/health)
if [ "$response" == "OK" ]; then
    echo -e "${GREEN}✓ Health check passed${NC}"
else
    echo -e "${RED}✗ Health check failed${NC}"
    exit 1
fi
echo ""

# Test 2: Create wallet
echo "Test 2: Create Wallet"
create_response=$(curl -s -X POST $BASE_URL/wallets \
  -H "Content-Type: application/json" \
  -d '{"user_id": "test_user"}')

wallet_id=$(echo $create_response | grep -o '"id":"[^"]*"' | cut -d'"' -f4)

if [ -n "$wallet_id" ]; then
    echo -e "${GREEN}✓ Wallet created: $wallet_id${NC}"
else
    echo -e "${RED}✗ Failed to create wallet${NC}"
    echo "Response: $create_response"
    exit 1
fi
echo ""

# Test 3: Fund wallet
echo "Test 3: Fund Wallet"
fund_response=$(curl -s -X POST $BASE_URL/wallets/$wallet_id/fund \
  -H "Content-Type: application/json" \
  -d '{"amount": "100.00"}')

balance=$(echo $fund_response | grep -o '"balance":"[^"]*"' | cut -d'"' -f4)

if [ "$balance" == "100.0000" ]; then
    echo -e "${GREEN}✓ Wallet funded successfully. Balance: $balance${NC}"
else
    echo -e "${RED}✗ Failed to fund wallet${NC}"
    echo "Response: $fund_response"
    exit 1
fi
echo ""

# Test 4: Get wallet
echo "Test 4: Get Wallet Balance"
get_response=$(curl -s $BASE_URL/wallets/$wallet_id)
current_balance=$(echo $get_response | grep -o '"balance":"[^"]*"' | cut -d'"' -f4)

if [ "$current_balance" == "100.0000" ]; then
    echo -e "${GREEN}✓ Balance verified: $current_balance${NC}"
else
    echo -e "${RED}✗ Balance mismatch${NC}"
    exit 1
fi
echo ""

# Test 5: Create second wallet and transfer
echo "Test 5: Transfer Money"
create_response2=$(curl -s -X POST $BASE_URL/wallets \
  -H "Content-Type: application/json" \
  -d '{"user_id": "test_user_2"}')

wallet_id2=$(echo $create_response2 | grep -o '"id":"[^"]*"' | cut -d'"' -f4)

transfer_response=$(curl -s -X POST $BASE_URL/wallets/$wallet_id/transfer \
  -H "Content-Type: application/json" \
  -d "{\"to_wallet_id\": \"$wallet_id2\", \"amount\": \"30.00\"}")

if echo $transfer_response | grep -q '"success":true'; then
    echo -e "${GREEN}✓ Transfer completed${NC}"
else
    echo -e "${RED}✗ Transfer failed${NC}"
    echo "Response: $transfer_response"
    exit 1
fi
echo ""

# Verify final balances
echo "Test 6: Verify Final Balances"
balance1=$(curl -s $BASE_URL/wallets/$wallet_id | grep -o '"balance":"[^"]*"' | cut -d'"' -f4)
balance2=$(curl -s $BASE_URL/wallets/$wallet_id2 | grep -o '"balance":"[^"]*"' | cut -d'"' -f4)

if [ "$balance1" == "70.0000" ] && [ "$balance2" == "30.0000" ]; then
    echo -e "${GREEN}✓ Wallet 1 balance: $balance1${NC}"
    echo -e "${GREEN}✓ Wallet 2 balance: $balance2${NC}"
else
    echo -e "${RED}✗ Balance verification failed${NC}"
    echo "Wallet 1: $balance1 (expected 70.0000)"
    echo "Wallet 2: $balance2 (expected 30.0000)"
    exit 1
fi
echo ""

echo "============================================"
echo -e "${GREEN}🎉 All tests passed!${NC}"
echo "============================================"
echo ""
echo "Your wallet service is working correctly!"
echo ""
echo "Test wallets created:"
echo "  Wallet 1: $wallet_id (Balance: $70)"
echo "  Wallet 2: $wallet_id2 (Balance: $30)"