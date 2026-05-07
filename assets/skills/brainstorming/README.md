# Brainstorming — Autonomous Decision Branching

## Purpose
When the agent hits a decision point or roadblock, this skill generates branching options, evaluates them objectively, selects the best path forward, and tracks previously-tried directions to prevent looping. Designed for fully autonomous operation — no user input required.

## When to Use
- Agent needs to decide between multiple implementation approaches
- Current approach has stalled, failed, or produced no progress
- Task requires creative problem-solving with multiple possible paths
- One Shot autonomous mode (called automatically at each cycle and at every roadblock)
- Any situation where the agent must choose a direction without user guidance

## Protocol

### Step 1: Analyze Current State
- What was the original task?
- What has been tried so far? List concrete actions taken.
- What worked? What failed? Why?
- What is the current blocker or decision point?

### Step 2: Generate Branching Options
- Produce 3-5 distinct, creative directions
- Each option must be concrete and actionable (not "try harder")
- Options should differ meaningfully in approach, not just surface details
- Include at least one unconventional or lateral-thinking option

### Step 3: Evaluate Each Option
- Rate each option on: feasibility (0-1), expected impact (0-1), risk (0-1)
- Score novelty against the previously-tried list (0.0 = identical to tried, 1.0 = completely new)
- List concrete pros and cons for each

### Step 4: Select Best Option
- Choose the option with the best balance of novelty + feasibility + impact
- Provide a clear rationale for the selection
- If all options have novelty < 0.3, declare exhaustion

### Step 5: Output Structured Result

Output ONLY a JSON object (no markdown fences, no explanation):

```json
{
  "cycle": 1,
  "previously_tried": ["write a python script", "use bash one-liner"],
  "options": [
    {
      "direction": "Implement as a Rust binary with clap argument parsing",
      "pros": ["Fast execution", "Type safety", "Single binary"],
      "cons": ["Requires Rust toolchain", "Longer compile time"],
      "feasibility": 0.9,
      "impact": 0.8,
      "risk": 0.2,
      "novelty_score": 0.85
    }
  ],
  "selected": {
    "direction": "Implement as a Rust binary with clap argument parsing",
    "rationale": "Highest novelty (0.85) with strong feasibility (0.9). Rust's type system prevents runtime errors and clap provides mature CLI handling."
  }
}
```

If NO viable new options exist (all novelty scores < 0.3, or genuinely out of ideas), output:
```json
{
  "exhausted": true,
  "reason": "All 5 approaches (Python script, bash, Rust binary, Node.js, Go) have been tried. No remaining distinct approaches with acceptable feasibility."
}
```

## Key Constraints
- NEVER repeat a previously-tried direction verbatim or with cosmetic changes
- Novelty score < 0.3 means the option is essentially a repeat — exclude it from options unless it's the only one
- Maximum 5 options per cycle (3 minimum)
- Selection must include explicit rationale referencing the scores
- Be honest about exhaustion — declaring it early saves time; forcing bad options wastes turns
- Each direction must be a complete, standalone approach — not a micro-step
- Prefer directions that can be executed in < 20 turns
