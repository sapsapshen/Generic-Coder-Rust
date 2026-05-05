# Create Skill — Meta-Skill for Crafting New Agent Skills

## Purpose
Guide the agent through the process of creating new, reusable skill definitions. This is the skill that teaches the agent *how to make skills* — a meta-skill.

## When to Use
- User asks to "create a skill for X"
- You discover a repeatable workflow that should be codified as a skill
- User wants to automate a multi-step process
- You find yourself doing the same pattern across multiple sessions

## Skill File Structure
Each skill lives in `skills/<skill-name>/` and requires:
- `README.md` — The skill definition with Purpose, When to Use, How It Works, Usage Pattern, and Key Constraints

## Creation Workflow
1. **Identify the pattern**: What task repeats? What are the entry conditions?
2. **Name the skill**: Use kebab-case, descriptive (e.g., `deploy-docker`, `db-migration`)
3. **Write README.md** with these sections:
   - `## Purpose` — One sentence on what it does
   - `## When to Use` — Bullet list of trigger conditions
   - `## How It Works` — Step-by-step workflow
   - `## Usage Pattern` — Code/tool-call template
   - `## Key Constraints` — Gotchas, limits, safety rules
4. **Register**: Create the directory, write README.md, save to `skills/<name>/`
5. **Verify**: Check the skill appears in the Skills panel and the agent can read it

## Key Constraints
- Skill files should be concise (under 200 lines)
- Focus on *when* and *how*, not exhaustive documentation
- The Purpose section is the most critical — it determines matching
- Test the skill immediately after creating it
