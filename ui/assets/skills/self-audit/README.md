# Self Audit — Agent Self-Reflection & Improvement

## Purpose
Pause and audit the agent's own performance: what's working, what's not, what should change. Use this skill when the task is going off-track, taking too long, or when the user signals dissatisfaction.

## When to Use
- Multiple consecutive failures on the same approach
- You've been going in circles (same tool calls repeating)
- The user says "this isn't working" or "try a different approach"
- A task has taken more than 10 turns without clear progress
- Before asking the user for help (audit yourself first)
- After completing a complex task (retrospective for learning)

## How It Works
1. **Stop**: Pause all tool calls. Don't try the next thing yet.
2. **Re-read**: Go back to the original task description. What was actually asked?
3. **Audit history**: Review the last N turns. What worked? What failed? Why?
4. **Identify the gap**: Is it a tool limitation? Wrong approach? Missing information?
5. **Decide**:
   - **Pivot**: Switch to a different approach
   - **Probe**: Gather more information before retrying
   - **Simplify**: Reduce scope, ship a partial solution
   - **Escalate**: Ask the user with specific, actionable questions
6. **Document**: If you learned something reusable, write to working memory

## Usage Pattern
```
Step 1: Re-read original task description
Step 2: Review recent turns in conversation history
Step 3: Identify pattern of failures or stalls
Step 4: Update working memory with findings
Step 5: Choose pivot/probe/simplify/escalate
Step 6: Act on the decision
```

## Key Constraints
- Audit before asking the user — don't escalate without self-diagnosis first
- Be honest about failures — hiding them wastes more time
- The audit itself should be quick (1-2 turns max)
- Don't audit prematurely — give approaches at least 3 attempts before pivoting
- Record learnings in working memory so they persist across turns
