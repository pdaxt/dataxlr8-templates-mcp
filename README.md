# dataxlr8-templates-mcp

Template management MCP for DataXLR8 — create, manage, and render email, proposal, invoice, and report templates with variable substitution and usage tracking.

## Tools

| Tool | Description |
|------|-------------|
| create_template | Create a new template with {{variable}} placeholders |
| render_template | Render a template by replacing {{variable}} placeholders with provided values |
| list_templates | List templates with optional category filter and pagination |
| get_template | Get a single template by ID or name |
| update_template | Update a template's body, category, or metadata |
| delete_template | Delete a template and its usage log |
| clone_template | Duplicate an existing template with a new name |
| template_usage | Get usage statistics for a template including recent render log |

## Setup

```bash
DATABASE_URL=postgres://dataxlr8:dataxlr8@localhost:5432/dataxlr8 cargo run
```

## Schema

Creates `templates.*` schema in PostgreSQL with tables for:
- `templates` — template definitions with categories (email, proposal, invoice, report), body, and variables
- `usage_log` — render history and variable usage tracking

## Part of

[DataXLR8](https://github.com/pdaxt) - AI-powered recruitment platform
