#!/bin/bash

echo "Building solXEN minting on X1..."

# Create necessary directories
mkdir -p database
mkdir -p burn-data

# Build the project
cargo build --release

echo "Build completed successfully!"
echo ""
echo "Usage:"
echo "  ./target/release/x1-solxen run        # Run full pipeline"
echo "  ./target/release/x1-solxen migrate    # Migrate data only"
echo "  ./target/release/x1-solxen mint       # Process minting only"
echo "  ./target/release/x1-solxen generate   # Generate HTML only"
echo ""
echo "Make sure to:"
echo "1. Place burns.db file in the burn-data/ directory"
echo "2. Ensure ~/.config/solana/id.json exists with your keypair"
echo "3. Check the generated index.html file"