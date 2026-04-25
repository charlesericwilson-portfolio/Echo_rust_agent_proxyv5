# Contributing to Echo_tmux_agentv3

## Current Challenges

### FIFO-Based Session Capture

**Problem:** The current `send-keys` + `pipe-pane` approach works for most tools, but `nmap` (and potentially other long-running or interactive tools) doesn't behave reliably inside sessions. Output can be incomplete, garbled, or missing entirely.

**Goal:** Move to a proper FIFO-based architecture where:
- Commands are sent via FIFO to the tmux session
- Output is captured cleanly via FIFO (no `capture-pane` snapshots)
- Markers (`===ECHO_START_===` / `===ECHO_END_===`) are used to reliably extract command output
- Sessions remain persistent and don't die unexpectedly

**Why this matters:** Proper FIFO support would make `SESSION:` calls reliable for all tools, including nmap, Metasploit, and other interactive/long-running processes.

**Skills needed:** Rust, tokio, tmux, named pipes (FIFO), async I/O

If you're interested in tackling this, open an issue or start a discussion!

Thanks for wanting to help! We appreciate all contributions. Here's the quick guide:

1. **Fork** this repo.
2. **Clone** your fork: `git clone https://github.com/charlesericwilson-portfolio/Echo_tmux_agentv3`
3. **Branch**: Create new branch: `git checkout -b fix-or-feature`
4. **Commit**: Clean messages, follow PEP 8 for Python
5. **Push**: `git push origin fix-or-feature`
6. **PR**: Go to your fork and create PR

We love code fixes, new tools, docs updates, or ideas.

Thanks!
