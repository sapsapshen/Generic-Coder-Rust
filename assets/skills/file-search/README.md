# File Search — Deep Codebase Exploration

## Purpose
Systematically search and explore codebases to understand structure, find definitions, trace dependencies, and locate relevant code. Use this skill for any non-trivial repository exploration.

## When to Use
- User asks "find where X is defined/used"
- You need to understand a project's architecture
- Searching for specific functions, classes, patterns across a codebase
- User asks "how does X work in this project?"
- You need to trace call chains or import graphs
- Before making changes to unfamiliar code

## How It Works
1. **Orient**: Use `workspace_list` or `workspace_search` to understand project structure
2. **Search broadly**: Use `content_search` with regex patterns for the target
3. **Narrow down**: Use `file_read` to inspect matches
4. **Trace dependencies**: Follow imports, function calls, class hierarchies
5. **Summarize**: Present findings with file paths and line numbers

## Usage Pattern
```
Step 1: workspace_list or workspace_search to find entry points
Step 2: content_search with pattern="function_name|class_name|pattern"
Step 3: file_read specific files for context
Step 4: (optional) content_search for callers/importers
Step 5: Summarize with file:line references
```

## Key Constraints
- Always read before modifying — don't assume file contents from search results alone
- Use specific regex patterns to minimize noise
- Search broadly first, then narrow — don't guess file locations
- Report findings with exact file paths and line numbers
- If the codebase is large, focus on the most relevant directories first
