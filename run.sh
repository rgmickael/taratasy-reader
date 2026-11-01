#!/bin/bash
# Quick run script for Taratasy Reader

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

BINARY="./target/release/taratasy-reader"
BUILD_DIR="./target/release"

echo -e "${BLUE}📖 Taratasy Reader${NC}"
echo "=================================="
echo ""

# Check if binary exists
if [ ! -f "$BINARY" ]; then
    echo -e "${YELLOW}⚠️  Binary not found. Building...${NC}"
    echo ""
    cargo build --release

    if [ $? -ne 0 ]; then
        echo -e "${RED}❌ Build failed!${NC}"
        exit 1
    fi
    echo ""
fi

# Check if binary was built successfully
if [ ! -f "$BINARY" ]; then
    echo -e "${RED}❌ Error: Binary not found at $BINARY${NC}"
    exit 1
fi

echo -e "${GREEN}✅ Starting Taratasy Reader...${NC}"
echo ""

# Run the app
exec "$BINARY"
