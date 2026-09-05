#!/usr/bin/env bash
set -euo pipefail

# Demo script for adjmerge — based on Apache Lucene PR #16378 conflict.
# Two independent CHANGES.txt entries inserted after the same anchor.
# Git conflicts. adjmerge resolves cleanly.
#
# Usage:
#   ./scripts/demo.sh setup                       # create demo repo (once)
#   ./scripts/demo.sh conflict kdiff3              # show conflict in kdiff3
#   ./scripts/demo.sh conflict meld                # show conflict in meld
#   ./scripts/demo.sh resolved kdiff3              # show clean merge in kdiff3
#   ./scripts/demo.sh resolved meld                # show clean merge in meld
#   ./scripts/demo.sh resolved terminal            # show clean merge in terminal

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
ADJMERGE="$PROJECT_DIR/target/release/adjmerge"
DEMO_DIR="/tmp/adjmerge_demo"
CASES="$PROJECT_DIR/tests/cases/lucene"

if [ ! -x "$ADJMERGE" ]; then
    echo "Build first: cargo build --release"
    exit 1
fi

setup_repo() {
    if [ -d "$DEMO_DIR/.git" ]; then
        cd "$DEMO_DIR"
        # Verify expected branches exist
        if git rev-parse --verify fix/singleton-docvalues --quiet >/dev/null 2>&1; then
            return
        fi
        # Repo exists but is stale — recreate
        rm -rf "$DEMO_DIR"
    fi
    mkdir -p "$DEMO_DIR"
    cd "$DEMO_DIR"
    git init -b main --quiet

    # Base: one existing entry (the anchor both sides insert after)
    cat > CHANGES.txt << 'EOF'
Improvements
---------------------

* GITHUB#16141: Optimize DefaultBulkScorer for ConstantScoreScorer by batching
  doc IDs into a FixedBitSet window via intoBitSet, replacing per-doc virtual
  dispatch with bulk bitwise operations. (Costin Leau)

Other
---------------------
EOF
    git add CHANGES.txt
    git commit --author="Costin Leau <costin@elastic.co>" \
        -m "CHANGES.txt baseline with #16141" --quiet

    # Branch: adds #16295 entry after the anchor
    git checkout -b fix/singleton-docvalues --quiet
    cat > CHANGES.txt << 'EOF'
Improvements
---------------------

* GITHUB#16141: Optimize DefaultBulkScorer for ConstantScoreScorer by batching
  doc IDs into a FixedBitSet window via intoBitSet, replacing per-doc virtual
  dispatch with bulk bitwise operations. (Costin Leau)

* GITHUB#16295: SingletonSortedNumericDocValues now delegates rangeIntoBitSet
  to the wrapped NumericDocValues, enabling optimized range evaluation for
  single-valued sorted numeric fields. (Costin Leau)

Other
---------------------
EOF
    git add CHANGES.txt
    git commit --author="Costin Leau <costin@elastic.co>" \
        -m "Add #16295 rangeIntoBitSet delegation entry" --quiet

    # Main: adds #16166 entry after the same anchor
    git checkout main --quiet
    cat > CHANGES.txt << 'EOF'
Improvements
---------------------

* GITHUB#16141: Optimize DefaultBulkScorer for ConstantScoreScorer by batching
  doc IDs into a FixedBitSet window via intoBitSet, replacing per-doc virtual
  dispatch with bulk bitwise operations. (Costin Leau)

* GITHUB#16166: Add scalar bulk range evaluation for sorted numeric doc values,
  letting skip-indexed multi-valued range queries participate in
  DenseConjunctionBulkScorer's bitset flow. (Costin Leau)

Other
---------------------
EOF
    git add CHANGES.txt
    git commit --author="Costin Leau <costin@elastic.co>" \
        -m "Add #16166 batch range evaluation entry" --quiet

    echo "Demo repo created at $DEMO_DIR"
}

reset_merge() {
    cd "$DEMO_DIR"
    git merge --abort 2>/dev/null || true
    git checkout main --quiet 2>/dev/null
    # Undo any previous merge commit back to the pre-merge state
    local branch_count
    branch_count=$(git rev-list --count main...fix/singleton-docvalues 2>/dev/null || echo "0")
    if git log --oneline -1 | grep -q "Merge"; then
        git reset --hard HEAD~1 --quiet
    fi
    rm -f .gitattributes
    git config --unset merge.adjmerge.name 2>/dev/null || true
    git config --unset merge.adjmerge.driver 2>/dev/null || true
    rm -f CHANGES.txt.orig CHANGES.txt_BACKUP_* CHANGES.txt_BASE_* CHANGES.txt_LOCAL_* CHANGES.txt_REMOTE_*
}

open_tool() {
    local tool="$1"
    case "$tool" in
        kdiff3)
            git mergetool --tool=kdiff3 --no-prompt
            ;;
        meld)
            git mergetool --tool=meld --no-prompt
            ;;
        terminal)
            echo ""
            cat CHANGES.txt
            ;;
        *)
            echo "Unknown tool: $tool (try kdiff3, meld, terminal)"
            exit 1
            ;;
    esac
}

show_conflict() {
    local tool="${1:-kdiff3}"
    setup_repo
    reset_merge

    echo "=== git merge (NO adjmerge) ==="
    git merge fix/singleton-docvalues 2>&1 || true

    if [ "$tool" = "terminal" ]; then
        echo ""
        echo "Conflict markers:"
        cat CHANGES.txt
    else
        echo ""
        echo "Opening $tool — take screenshot, then close."
        open_tool "$tool"
    fi
}

show_resolved() {
    local tool="${1:-terminal}"
    setup_repo
    reset_merge

    # Configure adjmerge as merge driver
    git config merge.adjmerge.name "adjmerge"
    git config merge.adjmerge.driver "$ADJMERGE --auto %O %A %B %A"
    echo '* merge=adjmerge' > .gitattributes

    echo "=== git merge (WITH adjmerge) ==="
    git merge fix/singleton-docvalues 2>&1

    if [ "$tool" = "terminal" ]; then
        echo ""
        echo "Merged result:"
        cat CHANGES.txt
    else
        echo ""
        echo "Opening $tool — take screenshot of clean merge, then close."
        # Show the resolved file in the tool against base for comparison
        git diff HEAD~1 -- CHANGES.txt | cat
        echo ""
        echo "Opening diff view..."
        case "$tool" in
            kdiff3)
                local base_file
                base_file=$(git rev-parse HEAD~1)
                git show "$base_file:CHANGES.txt" > /tmp/adjmerge_base.txt
                kdiff3 /tmp/adjmerge_base.txt CHANGES.txt &
                wait
                rm -f /tmp/adjmerge_base.txt
                ;;
            meld)
                local base_file
                base_file=$(git rev-parse HEAD~1)
                git show "$base_file:CHANGES.txt" > /tmp/adjmerge_base.txt
                meld /tmp/adjmerge_base.txt CHANGES.txt
                rm -f /tmp/adjmerge_base.txt
                ;;
        esac
    fi
}

case "${1:-help}" in
    setup)
        setup_repo
        ;;
    conflict)
        show_conflict "${2:-kdiff3}"
        ;;
    resolved)
        show_resolved "${2:-terminal}"
        ;;
    *)
        echo "Usage:"
        echo "  $0 setup                          # create demo repo (once)"
        echo "  $0 conflict [kdiff3|meld|terminal]"
        echo "  $0 resolved [kdiff3|meld|terminal]"
        echo ""
        echo "Workflow for screenshots:"
        echo "  1. $0 conflict kdiff3    → screenshot: conflict"
        echo "  2. $0 resolved kdiff3    → screenshot: clean merge"
        echo "  3. $0 conflict meld      → screenshot: conflict"
        echo "  4. $0 resolved meld      → screenshot: clean merge"
        ;;
esac
