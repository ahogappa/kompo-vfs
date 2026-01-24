#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

VERSION_FILE="$PROJECT_ROOT/VERSION"
CARGO_TOML="$PROJECT_ROOT/kompo_fs/Cargo.toml"
FORMULA_FILE="$PROJECT_ROOT/Formula/kompo-vfs.rb"

if [ -z "$1" ]; then
    echo "Usage: $0 <new_version>"
    echo "Example: $0 0.6.0"
    echo ""
    echo "Current version: $(cat "$VERSION_FILE")"
    exit 1
fi

NEW_VERSION="$1"

echo "Updating version to $NEW_VERSION..."

# Update VERSION file
echo "$NEW_VERSION" > "$VERSION_FILE"
echo "  Updated: VERSION"

# Update Cargo.toml
sed -i '' "s/^version = \".*\"/version = \"$NEW_VERSION\"/" "$CARGO_TOML"
echo "  Updated: kompo_fs/Cargo.toml"

# Update Formula
sed -i '' "s/^  version \".*\"/  version \"$NEW_VERSION\"/" "$FORMULA_FILE"
echo "  Updated: Formula/kompo-vfs.rb"

echo ""
echo "Done! All files updated to version $NEW_VERSION"
echo ""
echo "Verify changes:"
grep -H "version" "$VERSION_FILE" "$CARGO_TOML" "$FORMULA_FILE" | grep -E "(^VERSION|^version|\"$NEW_VERSION\")"
