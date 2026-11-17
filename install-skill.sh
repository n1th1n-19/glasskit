#!/bin/sh
# Install only the glasskit Claude skill (no desktop changes): copies skills/glasskit into ~/.claude/skills.
# One-liner: curl -fsSL https://raw.githubusercontent.com/n1th1n-19/glasskit/main/install-skill.sh | sh
set -eu
DEST=${CLAUDE_SKILLS_DIR:-$HOME/.claude/skills}/glasskit
BASE=https://raw.githubusercontent.com/n1th1n-19/glasskit/main/skills/glasskit
mkdir -p "$DEST/references"
for f in SKILL.md references/plasma.md references/porting.md references/design.md; do
    curl -fsSL "$BASE/$f" -o "$DEST/$f"
done
echo "glasskit skill installed in $DEST (start a new Claude Code session to load it)"
