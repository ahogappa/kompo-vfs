#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

CARGO_TOML="$PROJECT_ROOT/Cargo.toml"
FORMULA_FILE="$PROJECT_ROOT/Formula/kompo-vfs.rb"

CURRENT_VERSION=$(sed -n '/\[workspace\.package\]/,/^\[/{ s/^version = "\(.*\)"/\1/p; }' "$CARGO_TOML")

if [ -z "$1" ]; then
    echo "Usage: $0 <new_version>"
    echo "Example: $0 0.7.0"
    echo ""
    echo "Current version: $CURRENT_VERSION"
    exit 1
fi

NEW_VERSION="$1"

echo "Updating version to $NEW_VERSION..."

# Update workspace.package.version in root Cargo.toml
sed -i '' "/\[workspace\.package\]/,/^\[/ s/^version = \".*\"/version = \"$NEW_VERSION\"/" "$CARGO_TOML"
echo "  Updated: Cargo.toml (workspace.package.version)"

# Update Formula
sed -i '' "s/^  version \".*\"/  version \"$NEW_VERSION\"/" "$FORMULA_FILE"
echo "  Updated: Formula/kompo-vfs.rb"

echo ""
echo "Done! All files updated to version $NEW_VERSION"
echo ""
echo "Verify changes:"
echo "  Cargo.toml workspace version: $(sed -n '/\[workspace\.package\]/,/^\[/{ s/^version = "\(.*\)"/\1/p; }' "$CARGO_TOML")"
echo "  Formula version: $(grep 'version "' "$FORMULA_FILE" | head -1 | sed 's/.*version "\(.*\)"/\1/')"
