# Code Review — Systematic Code Review Workflow

## Purpose
Perform a thorough, structured code review of changes — whether your own (before committing) or the user's (PR review). Catch bugs, enforce best practices, and suggest improvements.

## When to Use
- User asks "review my code" or "review this PR"
- You've made significant changes and should self-review before committing
- User asks "is this code correct/safe/idiomatic?"
- Before merging or finalizing any non-trivial change
- User wants a second opinion on architecture or implementation choices

## How It Works
1. **Scope**: Identify which files changed (`git_status`, `git_diff`)
2. **Read**: Read each changed file fully (not just the diff — context matters)
3. **Check against these dimensions**:
   - **Correctness**: Does it do what was intended? Edge cases handled?
   - **Safety**: Error handling, null checks, resource cleanup, input validation
   - **Security**: Injection risks, exposed secrets, auth issues, path traversal
   - **Performance**: N+1 queries, unnecessary allocations, blocking in async contexts
   - **Style**: Follows project conventions, consistent naming, clear structure
   - **Maintainability**: Clear logic, reasonable function size, good comments
4. **Report**: Group findings by severity (Critical / Warning / Suggestion)
5. **Fix**: If it's your own code, apply fixes. If reviewing user code, ask before modifying

## Usage Pattern
```
Step 1: git_status to see what changed
Step 2: git_diff to see the diff
Step 3: file_read each changed file for full context
Step 4: content_search for related code (callers, imports)
Step 5: Output structured review with file:line references
```

## Key Constraints
- Always read full files, not just diffs — context reveals bugs
- Separate objective findings from subjective opinions
- Suggest concrete fixes, not vague criticisms
- When reviewing your own code, be extra critical of error handling
- Don't review files you haven't read
