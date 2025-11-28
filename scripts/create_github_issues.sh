#!/bin/bash
# Script to create GitHub issues for the modular architecture refactor
# Prerequisites: gh CLI authenticated with appropriate permissions

set -e

REPO="jamesjwu/tlparse"
ISSUES_DIR="docs/issues"
MAIN_ISSUE_FILE="docs/MODULAR_ARCHITECTURE_ISSUE.md"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}Creating GitHub issues for tlparse modular architecture refactor${NC}"
echo ""

# Check if gh is available
if ! command -v gh &> /dev/null; then
    echo -e "${RED}Error: gh CLI is not installed. Please install it first.${NC}"
    echo "  brew install gh  # macOS"
    echo "  sudo apt install gh  # Ubuntu/Debian"
    exit 1
fi

# Check if authenticated
if ! gh auth status &> /dev/null; then
    echo -e "${RED}Error: gh CLI is not authenticated. Please run 'gh auth login' first.${NC}"
    exit 1
fi

# Create the main tracking issue first
echo -e "${YELLOW}Creating main tracking issue...${NC}"
MAIN_TITLE="Modular Architecture: Module System Implementation"
MAIN_BODY=$(cat "$MAIN_ISSUE_FILE")

MAIN_ISSUE_URL=$(gh issue create \
    --repo "$REPO" \
    --title "$MAIN_TITLE" \
    --body "$MAIN_BODY" \
    --label "enhancement,architecture" \
    2>&1)

if [[ $? -eq 0 ]]; then
    echo -e "${GREEN}Created main issue: $MAIN_ISSUE_URL${NC}"
    MAIN_ISSUE_NUM=$(echo "$MAIN_ISSUE_URL" | grep -oE '[0-9]+$')
else
    echo -e "${RED}Failed to create main issue${NC}"
    exit 1
fi

echo ""
echo -e "${YELLOW}Creating sub-issues...${NC}"

# Array to store created issue numbers for linking
declare -a CREATED_ISSUES

# Create sub-issues from the issues directory
for issue_file in $(ls -1 "$ISSUES_DIR"/*.md | sort); do
    filename=$(basename "$issue_file")

    # Extract title from first line (# Title)
    title=$(head -1 "$issue_file" | sed 's/^# //')

    # Get the issue body (skip first line)
    body=$(tail -n +2 "$issue_file")

    # Add reference to main issue at the bottom
    body="$body

---
Parent issue: #$MAIN_ISSUE_NUM"

    echo -n "  Creating: $title... "

    ISSUE_URL=$(gh issue create \
        --repo "$REPO" \
        --title "$title" \
        --body "$body" \
        --label "enhancement" \
        2>&1)

    if [[ $? -eq 0 ]]; then
        ISSUE_NUM=$(echo "$ISSUE_URL" | grep -oE '[0-9]+$')
        CREATED_ISSUES+=("$ISSUE_NUM")
        echo -e "${GREEN}#$ISSUE_NUM${NC}"
    else
        echo -e "${RED}Failed${NC}"
    fi
done

echo ""
echo -e "${GREEN}Done! Created ${#CREATED_ISSUES[@]} sub-issues.${NC}"
echo ""
echo "Issue numbers created:"
echo "  Main: #$MAIN_ISSUE_NUM"
for num in "${CREATED_ISSUES[@]}"; do
    echo "  Sub:  #$num"
done

# Optionally update main issue with links to sub-issues
echo ""
echo -e "${YELLOW}Would you like to update the main issue with links to all sub-issues? (y/n)${NC}"
read -r response

if [[ "$response" =~ ^[Yy]$ ]]; then
    # Build the links section
    LINKS_SECTION="## Created Sub-Issues\n\n"
    for num in "${CREATED_ISSUES[@]}"; do
        LINKS_SECTION+="- #$num\n"
    done

    # Append to main issue
    gh issue comment "$MAIN_ISSUE_NUM" \
        --repo "$REPO" \
        --body "$(echo -e "$LINKS_SECTION")"

    echo -e "${GREEN}Updated main issue with sub-issue links${NC}"
fi

echo ""
echo -e "${GREEN}All done!${NC}"
