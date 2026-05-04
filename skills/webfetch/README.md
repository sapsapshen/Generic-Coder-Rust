# WebFetch — Web Content Retrieval & Analysis

## Purpose
Fetch and analyze content from any URL. Use this skill whenever the task requires reading web pages, API responses, documentation, or any HTTP-accessible resource.

## When to Use
- User asks you to read, check, or analyze a web page
- You need to look up documentation or API references online
- You need to fetch JSON/XML data from a REST API
- User provides a URL and asks "what's on this page?"
- You need to verify a claim by checking an online source
- Searching for package/library information on registries (npm, crates.io, pypi)

## How It Works
1. Use `code_run` with Python's `requests`/`httpx` to fetch the URL
2. Parse and extract relevant content
3. Summarize findings for the user

## Usage Pattern
```python
import requests
resp = requests.get(url, headers={"User-Agent": "GenericCoder/1.0"})
# Extract and analyze content
```

## Key Constraints
- Always set a reasonable User-Agent header
- Respect robots.txt conventions
- Handle timeouts gracefully (default 30s)
- For large pages, extract only the relevant sections
- Cache results within a session to avoid re-fetching
