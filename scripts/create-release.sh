#!/bin/bash

# Script to create a new release
# Usage: ./scripts/create-release.sh <version>
# Example: ./scripts/create-release.sh 1.0.1

set -e

if [ $# -eq 0 ]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 1.0.1"
    exit 1
fi

VERSION=$1

# Validate version format
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Error: Version must be in format X.Y.Z (e.g., 1.0.1)"
    exit 1
fi

echo "Creating release for version v$VERSION"

# Update version in Cargo.toml
echo "Updating Cargo.toml version..."
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml
else
    # Linux
    sed -i "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml
fi

# Update Cargo.lock
echo "Updating Cargo.lock..."
cargo update -p solana-validator-switch

# Run tests
echo "Running tests..."
cargo test

# Commit changes
echo "Committing changes..."
git add Cargo.toml Cargo.lock
git commit -m "chore: bump version to $VERSION"

# Create and push tag
echo "Creating tag v$VERSION..."
git tag -a "v$VERSION" -m "Release v$VERSION"

echo ""
echo "Release v$VERSION created successfully!"
echo ""
echo "To push the release:"
echo "  git push origin main"
echo "  git push origin v$VERSION"
echo ""
echo "This will trigger the GitHub Actions workflow to build and publish binaries."