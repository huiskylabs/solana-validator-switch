#!/bin/bash
# Setup script to install Git hooks

set -e

echo "🔧 Setting up Git hooks..."

# Create .git/hooks directory if it doesn't exist
mkdir -p .git/hooks

# Install pre-commit hook
if [ -f ".githooks/pre-commit" ]; then
    cp .githooks/pre-commit .git/hooks/pre-commit
    chmod +x .git/hooks/pre-commit
    echo "✅ Pre-commit hook installed"
else
    echo "❌ Pre-commit hook not found in .githooks/pre-commit"
    exit 1
fi

# Install cargo-audit if not present
if ! command -v cargo-audit &> /dev/null; then
    echo "📦 Installing cargo-audit for security checks..."
    cargo install cargo-audit
    echo "✅ cargo-audit installed"
else
    echo "✅ cargo-audit already installed"
fi

echo "🎉 Git hooks setup complete!"
echo ""
echo "ℹ️  The pre-commit hook will now run the following checks before each commit:"
echo "   - Code formatting (cargo fmt --check)"
echo "   - Clippy linting (cargo clippy)"
echo "   - Tests (cargo test)"
echo "   - Build (cargo build)"
echo "   - Security audit (cargo audit)"
echo ""
echo "💡 To bypass the pre-commit hook in emergencies, use:"
echo "   git commit --no-verify"
echo ""
echo "🔧 To manually run the pre-commit checks:"
echo "   .githooks/pre-commit"