# :page_facing_up: dataxlr8-templates-mcp

Template management for AI agents — create, render, clone, and track usage of email, proposal, and report templates.

[![Rust](https://img.shields.io/badge/Rust-2024_edition-orange?logo=rust)](https://www.rust-lang.org/)
[![MCP](https://img.shields.io/badge/MCP-rmcp_0.17-blue)](https://modelcontextprotocol.io/)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

## What It Does

Manages reusable templates with `{{variable}}` placeholders through MCP tool calls. Create templates for emails, proposals, invoices, and reports, render them with variable substitution, clone existing templates for variations, and track render usage over time. Supports categories, metadata, and full usage analytics — all backed by PostgreSQL.

## Architecture

```
                    ┌──────────────────────────┐
AI Agent ──stdio──▶ │  dataxlr8-templates-mcp  │
                    │  (rmcp 0.17 server)       │
                    └──────────┬───────────────┘
                               │ sqlx 0.8
                               ▼
                    ┌─────────────────────────┐
                    │  PostgreSQL              │
                    │  schema: templates       │
                    │  ├── templates           │
                    │  └── usage_log           │
                    └─────────────────────────┘
```

## Tools

| Tool | Description |
|------|-------------|
| `create_template` | Create a template with `{{variable}}` placeholders |
| `render_template` | Render a template by replacing variables with values |
| `list_templates` | List templates with optional category filter |
| `get_template` | Get a single template by ID or name |
| `update_template` | Update a template's body, category, or metadata |
| `delete_template` | Delete a template and its usage log |
| `clone_template` | Duplicate a template with a new name |
| `template_usage` | Get usage statistics and recent render log |

## Quick Start

```bash
git clone https://github.com/pdaxt/dataxlr8-templates-mcp
cd dataxlr8-templates-mcp
cargo build --release

export DATABASE_URL=postgres://user:pass@localhost:5432/dataxlr8
./target/release/dataxlr8-templates-mcp
```

The server auto-creates the `templates` schema and all tables on first run.

## Configuration

| Variable | Required | Description |
|----------|----------|-------------|
| `DATABASE_URL` | Yes | PostgreSQL connection string |
| `LOG_LEVEL` | No | Tracing level (default: `info`) |

## Claude Desktop Integration

Add to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "dataxlr8-templates": {
      "command": "./target/release/dataxlr8-templates-mcp",
      "env": {
        "DATABASE_URL": "postgres://user:pass@localhost:5432/dataxlr8"
      }
    }
  }
}
```

## Part of DataXLR8

One of 14 Rust MCP servers that form the [DataXLR8](https://github.com/pdaxt) platform — a modular, AI-native business operations suite. Each server owns a single domain, shares a PostgreSQL instance, and communicates over the Model Context Protocol.

## License

MIT
