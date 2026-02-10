# Agent Instructions

This project uses **Linear** (via MCP tools) for issue tracking. All agents must use the Linear MCP tools
to create, update, and close issues.

## Quick Reference

```
list_issues                — Find available work (filter by project, state="Todo")
get_issue                  — View issue details
update_issue               — Claim work (set state to "In Progress")
update_issue               — Complete work (set state to "Done")
create_comment             — Add completion summary
```

## Session Initialization

At the start of every session, perform these steps before any work:

1. **Detect repository and branch context:**
   - Run `git remote get-url origin` to get the remote URL, then parse the repository name
     (e.g., `dotfiles.vorpal` from `github.com/ALT-F4-LLC/dotfiles.vorpal.git`)
   - Run `git branch --show-current` to get the current branch (e.g., `main`)
   - Alternatively, parse from the working directory path (e.g., `dotfiles.vorpal.git/main`)

2. **Look up the "Agents" team:**
   - Call `list_teams` and find the team named "Agents". Store its team name or ID.

3. **Look up the project matching the repository:**
   - Call `list_projects` and find the project matching the repository name.
   - If no matching project exists, create one using
     `create_project(team="Agents", name="<repository-name>")`.

4. **Look up available labels:**
   - Call `list_issue_labels` and confirm these labels exist: **"Bug"**, **"Feature"**, **"Improvement"**.

5. **Look up workflow states:**
   - Call `list_issue_statuses(team="Agents")` to get the available statuses (e.g., "Todo",
     "In Progress", "Done").

## Title Format Convention

All issue titles MUST follow this format:

```
[<branch>] <description>
```

Examples:
- `[main] Feature: add OAuth2 support`
- `[main] Bug: fix race condition in event handler`
- `[main] Explore: current authentication implementation`

## Scoping Rules

- **ONLY work with issues in the project matching the current repository.**
- **ONLY create or modify issues with the `[<branch>]` prefix matching the current branch.**
- When listing issues, always filter by project and scan results for the matching branch prefix.
- Never modify or interact with issues belonging to other projects or branches.

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until all
issues are closed in Linear.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Close finished work** - `update_issue(id, state="Done")` for each completed issue
4. **Add completion summaries** - `create_comment(issueId, body="Completed: summary")` for each closed issue
5. **Verify** - All changes committed AND pushed
6. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `update_issue` to "Done" succeeds for all finished issues
- NEVER stop before closing issues — that leaves work stranded with no record of completion

## Linear MCP Tool Reference

```
# Session setup
list_teams                         — Find the "Agents" team
list_projects                      — Find/verify the repository project
create_project                     — Create a new project (team, name)
list_issue_labels                  — Get available labels (Bug, Feature, Improvement)
list_issue_statuses                — Get available statuses (Todo, In Progress, Done)

# Check existing state
list_issues                        — Search issues (filter by project, state, assignee, query)
get_issue                          — Full details of a specific issue

# Create issues
create_issue                       — Create issue (team, title, description, priority, parentId, project, labels, blocks, blockedBy)

# Update issues
update_issue                       — Update state, priority, title, description, labels, blocks, blockedBy
create_comment                     — Add comments for context/updates
```

### Priorities

| Priority | Meaning |
|---|---|
| 1 | Urgent |
| 2 | High |
| 3 | Medium (default) |
| 4 | Low |
| 0 | No priority / Backlog |

### Labels

Every issue must have exactly one of these labels:

| Label | Use When |
|---|---|
| **Bug** | Fixing broken behavior, errors, regressions |
| **Feature** | Adding new functionality |
| **Improvement** | Refactoring, chores, tasks, documentation, performance |
