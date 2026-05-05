# CLI Anything — Convert natural language to shell commands

## Purpose
Translate user's natural language descriptions into precise, safe shell commands. Execute them and report results. The user can describe what they want in plain language without knowing the exact command syntax.

## When to Use
- User asks to do something on the command line (e.g., "find all PDF files modified this week", "kill the process using port 3000")
- User describes a system operation (e.g., "check disk usage", "show me what's listening on port 8765")
- User wants to automate a repetitive terminal task
- User needs to install a package or tool
- User asks for system information (memory, CPU, network, etc.)
- Any request that maps naturally to a shell command or series of commands

## How It Works
1. **Understand**: Parse the user's intent from their natural language description
2. **Choose command**: Select the appropriate CLI tool(s) for the task
3. **Preview**: Show the command before executing, so the user can verify
4. **Execute**: Run with `code_run` (type: python or shell) or explain the command
5. **Report**: Present results clearly with context

## Usage Pattern
```
Step 1: Understand what the user wants to accomplish
Step 2: Identify the right CLI tool (find, grep, ls, ps, kill, df, du, curl, git, etc.)
Step 3: Construct a safe, correct command
Step 4: Execute or propose the command
Step 5: Format output for readability
```

## Guidelines
- **macOS/Linux first**: Prefer POSIX-compatible commands; note platform-specific alternatives
- **Safety check**: Add guards (`--dry-run`, `-i` confirmations) for destructive operations
- **Explain flags**: Include brief comments on non-obvious flags
- **Piping**: Use pipes (`|`) and filters (`grep`, `awk`, `head`) to focus output
- **Path quoting**: Always quote paths with spaces
- **Permissions**: Warn if the command likely needs `sudo`

## Common Patterns
- File finding: `find` with `-name`, `-mtime`, `-size`, `-type`
- Content search: `grep -r`, `rg` (ripgrep), `ag` (silver searcher)
- Process management: `ps aux | grep`, `lsof -i :PORT`, `kill`
- Disk: `df -h`, `du -sh *`, `ncdu`
- Network: `curl`, `ping`, `dig`, `netstat -an`, `ifconfig`
- System info: `uname -a`, `sw_vers` (macOS), `lsb_release -a` (Linux)
- Package management: `brew install` (macOS), `apt install` (Ubuntu), `pip install` (Python)
