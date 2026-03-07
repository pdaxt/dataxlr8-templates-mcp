use dataxlr8_mcp_core::mcp::{error_result, get_str, get_i64, json_result, make_schema};
use dataxlr8_mcp_core::Database;
use regex::Regex;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::ServerHandler;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

// ============================================================================
// Constants
// ============================================================================

const MAX_NAME_LEN: usize = 200;
const MAX_BODY_LEN: usize = 100_000;
const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;
const MAX_QUERY_LEN: usize = 500;
const VALID_CATEGORIES: &[&str] = &["email", "proposal", "invoice", "report"];

// ============================================================================
// Data types
// ============================================================================

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub category: String,
    pub body: String,
    pub variables: Vec<String>,
    pub metadata: serde_json::Value,
    pub usage_count: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct UsageLogEntry {
    pub id: String,
    pub template_id: String,
    pub rendered_at: chrono::DateTime<chrono::Utc>,
    pub variables_used: serde_json::Value,
}
// ============================================================================
// Input validation helpers
// ============================================================================

/// Extract a string param, trim it, and reject if empty.
fn require_trimmed_str(args: &serde_json::Value, key: &str) -> Result<String, CallToolResult> {
    match get_str(args, key) {
        Some(v) => {
            let trimmed = v.trim().to_string();
            if trimmed.is_empty() {
                Err(error_result(&format!(
                    "Parameter '{key}' must not be empty or whitespace-only"
                )))
            } else {
                Ok(trimmed)
            }
        }
        None => Err(error_result(&format!(
            "Missing required parameter: {key}"
        ))),
    }
}

/// Extract an optional string param and trim it. Returns None if missing.
fn optional_trimmed_str(args: &serde_json::Value, key: &str) -> Option<String> {
    get_str(args, key).map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// Validate that a category value is one of the allowed values.
fn validate_category(category: &str) -> Result<(), CallToolResult> {
    if !VALID_CATEGORIES.contains(&category) {
        return Err(error_result(&format!(
            "Invalid category '{}'. Must be one of: {}",
            category,
            VALID_CATEGORIES.join(", ")
        )));
    }
    Ok(())
}

/// Clamp limit to [1, MAX_LIMIT], default to given default_val.
fn parse_limit(args: &serde_json::Value, default_val: i64) -> i64 {
    get_i64(args, "limit")
        .unwrap_or(default_val)
        .max(1)
        .min(MAX_LIMIT)
}

/// Parse offset, default 0, clamp to >= 0.
fn parse_offset(args: &serde_json::Value) -> i64 {
    get_i64(args, "offset").unwrap_or(0).max(0)
}

/// Validate that metadata, if provided, is a JSON object.
fn validate_metadata(
    args: &serde_json::Value,
    fallback: serde_json::Value,
) -> Result<serde_json::Value, CallToolResult> {
    match args.get("metadata") {
        Some(v) if v.is_object() => Ok(v.clone()),
        Some(serde_json::Value::Null) | None => Ok(fallback),
        Some(_) => Err(error_result("Parameter 'metadata' must be a JSON object")),
    }
}

/// Extract and validate a template_id parameter (required, trimmed, length-checked).
fn require_template_id(args: &serde_json::Value) -> Result<String, CallToolResult> {
    let id = require_trimmed_str(args, "template_id")?;
    if id.len() > MAX_NAME_LEN {
        return Err(error_result(&format!(
            "Parameter 'template_id' too long ({} chars). Maximum is {MAX_NAME_LEN}.",
            id.len()
        )));
    }
    Ok(id)
}

// ============================================================================
// Tool definitions
// ============================================================================

fn build_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "create_template".into(),
            title: None,
            description: Some(
                "Create a new template with {{variable}} placeholders".into(),
            ),
            input_schema: make_schema(
                serde_json::json!({
                    "name": { "type": "string", "description": "Unique template name" },
                    "category": { "type": "string", "enum": ["email", "proposal", "invoice", "report"], "description": "Template category (default: email)" },
                    "body": { "type": "string", "description": "Template body with {{variable}} placeholders" },
                    "metadata": { "type": "object", "description": "Optional metadata (JSONB)" }
                }),
                vec!["name", "body"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "render_template".into(),
            title: None,
            description: Some(
                "Render a template by replacing {{variable}} placeholders with provided values"
                    .into(),
            ),
            input_schema: make_schema(
                serde_json::json!({
                    "template_id": { "type": "string", "description": "Template ID or name" },
                    "variables": { "type": "object", "description": "Map of variable name to value for substitution" }
                }),
                vec!["template_id", "variables"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "list_templates".into(),
            title: None,
            description: Some("List templates with optional category filter and pagination".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "category": { "type": "string", "enum": ["email", "proposal", "invoice", "report"], "description": "Filter by category" },
                    "limit": { "type": "integer", "description": "Max results to return (default: 50, max: 200)" },
                    "offset": { "type": "integer", "description": "Number of results to skip (default: 0)" }
                }),
                vec![],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "get_template".into(),
            title: None,
            description: Some("Get a single template by ID or name".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "template_id": { "type": "string", "description": "Template ID or name" }
                }),
                vec!["template_id"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "update_template".into(),
            title: None,
            description: Some("Update a template's body, category, or metadata".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "template_id": { "type": "string", "description": "Template ID or name" },
                    "body": { "type": "string", "description": "New template body" },
                    "category": { "type": "string", "enum": ["email", "proposal", "invoice", "report"], "description": "New category" },
                    "metadata": { "type": "object", "description": "New metadata (replaces existing)" }
                }),
                vec!["template_id"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "delete_template".into(),
            title: None,
            description: Some("Delete a template and its usage log".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "template_id": { "type": "string", "description": "Template ID or name" }
                }),
                vec!["template_id"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "clone_template".into(),
            title: None,
            description: Some("Duplicate an existing template with a new name".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "template_id": { "type": "string", "description": "Source template ID or name" },
                    "new_name": { "type": "string", "description": "Name for the cloned template" }
                }),
                vec!["template_id", "new_name"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "template_usage".into(),
            title: None,
            description: Some(
                "Get usage statistics for a template including recent render log".into(),
            ),
            input_schema: make_schema(
                serde_json::json!({
                    "template_id": { "type": "string", "description": "Template ID or name" },
                    "limit": { "type": "integer", "description": "Max recent usage entries to return (default: 50, max: 200)" },
                    "offset": { "type": "integer", "description": "Number of entries to skip (default: 0)" }
                }),
                vec!["template_id"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "search_templates".into(),
            title: None,
            description: Some("Search templates by name or body content with pagination".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "query": { "type": "string", "description": "Text to search for in template name and body (case-insensitive)" },
                    "category": { "type": "string", "enum": ["email", "proposal", "invoice", "report"], "description": "Optionally filter by category" },
                    "limit": { "type": "integer", "description": "Max results to return (default: 50, max: 200)" },
                    "offset": { "type": "integer", "description": "Number of results to skip (default: 0)" }
                }),
                vec!["query"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
    ]
}

// ============================================================================
// Helper: extract {{variable}} names from template body
// ============================================================================

fn extract_variables(body: &str) -> Vec<String> {
    let re = Regex::new(r"\{\{(\w+)\}\}").unwrap();
    let mut vars: Vec<String> = re
        .captures_iter(body)
        .map(|cap| cap[1].to_string())
        .collect();
    vars.sort();
    vars.dedup();
    vars
}

// ============================================================================
// MCP Server
// ============================================================================

#[derive(Clone)]
pub struct TemplatesMcpServer {
    db: Database,
}

impl TemplatesMcpServer {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Look up a template by ID or name.
    async fn find_template(&self, id_or_name: &str) -> Result<Option<Template>, String> {
        match sqlx::query_as::<_, Template>(
            "SELECT * FROM templates.templates WHERE id = $1 OR name = $1",
        )
        .bind(id_or_name)
        .fetch_optional(self.db.pool())
        .await
        {
            Ok(t) => Ok(t),
            Err(e) => {
                error!(id_or_name = id_or_name, error = %e, "Database lookup failed");
                Err(format!("Database error: {e}"))
            }
        }
    }

    // ---- Tool handlers ----

    async fn handle_create_template(&self, args: &serde_json::Value) -> CallToolResult {
        let name = match require_trimmed_str(args, "name") {
            Ok(n) => n,
            Err(e) => return e,
        };
        let body = match require_trimmed_str(args, "body") {
            Ok(b) => b,
            Err(e) => return e,
        };
        let category = optional_trimmed_str(args, "category").unwrap_or_else(|| "email".into());

        // Validate name length
        if name.len() > MAX_NAME_LEN {
            return error_result(&format!(
                "Template name too long ({} chars). Maximum is {MAX_NAME_LEN}.",
                name.len()
            ));
        }

        // Validate body length
        if body.len() > MAX_BODY_LEN {
            return error_result(&format!(
                "Template body too long ({} chars). Maximum is {MAX_BODY_LEN}.",
                body.len()
            ));
        }

        // Validate category
        if let Err(e) = validate_category(&category) {
            return e;
        }

        let metadata = match validate_metadata(args, serde_json::json!({})) {
            Ok(m) => m,
            Err(e) => return e,
        };

        let variables = extract_variables(&body);
        let id = uuid::Uuid::new_v4().to_string();

        match sqlx::query_as::<_, Template>(
            "INSERT INTO templates.templates (id, name, category, body, variables, metadata) VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
        )
        .bind(&id)
        .bind(&name)
        .bind(&category)
        .bind(&body)
        .bind(&variables)
        .bind(&metadata)
        .fetch_one(self.db.pool())
        .await
        {
            Ok(template) => {
                info!(name = %name, category = %category, id = %id, "Created template");
                json_result(&template)
            }
            Err(e) => {
                error!(name = %name, error = %e, "Failed to create template");
                if e.to_string().contains("unique") || e.to_string().contains("duplicate") {
                    error_result(&format!("A template named '{name}' already exists"))
                } else {
                    error_result(&format!("Failed to create template: {e}"))
                }
            }
        }
    }

    async fn handle_render_template(&self, args: &serde_json::Value) -> CallToolResult {
        let template_id = match require_template_id(args) {
            Ok(t) => t,
            Err(e) => return e,
        };

        let variables = match args.get("variables") {
            Some(v) if v.is_object() => v.clone(),
            Some(_) => return error_result("Parameter 'variables' must be a JSON object"),
            None => return error_result("Missing required parameter: variables"),
        };

        let template = match self.find_template(&template_id).await {
            Ok(Some(t)) => t,
            Ok(None) => return error_result(&format!("Template '{template_id}' not found")),
            Err(e) => return error_result(&e),
        };

        // Render: replace all {{variable}} with values
        let mut rendered = template.body.clone();
        if let Some(obj) = variables.as_object() {
            for (key, value) in obj {
                let placeholder = format!("{{{{{key}}}}}");
                let replacement = match value {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                rendered = rendered.replace(&placeholder, &replacement);
            }
        }

        // Detect unreplaced variables
        let unreplaced: Vec<String> = {
            let re = Regex::new(r"\{\{(\w+)\}\}").unwrap();
            re.captures_iter(&rendered)
                .map(|cap| cap[1].to_string())
                .collect()
        };
        if !unreplaced.is_empty() {
            warn!(
                template = %template.name,
                unreplaced = ?unreplaced,
                "Template rendered with unreplaced variables"
            );
        }

        // Log usage
        let log_id = uuid::Uuid::new_v4().to_string();
        if let Err(e) = sqlx::query(
            "INSERT INTO templates.usage_log (id, template_id, variables_used) VALUES ($1, $2, $3)",
        )
        .bind(&log_id)
        .bind(&template.id)
        .bind(&variables)
        .execute(self.db.pool())
        .await
        {
            error!(template_id = %template.id, error = %e, "Failed to log usage");
        }

        // Increment usage_count
        if let Err(e) = sqlx::query(
            "UPDATE templates.templates SET usage_count = usage_count + 1, updated_at = now() WHERE id = $1",
        )
        .bind(&template.id)
        .execute(self.db.pool())
        .await
        {
            error!(template_id = %template.id, error = %e, "Failed to increment usage count");
        }

        info!(template = %template.name, "Rendered template");
        json_result(&serde_json::json!({
            "template_name": template.name,
            "rendered": rendered,
            "variables_used": variables,
            "unreplaced_variables": unreplaced
        }))
    }

    async fn handle_list_templates(&self, args: &serde_json::Value) -> CallToolResult {
        let category = optional_trimmed_str(args, "category");
        let limit = parse_limit(args, DEFAULT_LIMIT);
        let offset = parse_offset(args);

        // Validate category if provided
        if let Some(ref cat) = category {
            if let Err(e) = validate_category(cat) {
                return e;
            }
        }

        let templates: Vec<Template> = if let Some(cat) = &category {
            match sqlx::query_as(
                "SELECT * FROM templates.templates WHERE category = $1 ORDER BY name LIMIT $2 OFFSET $3",
            )
            .bind(cat)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.pool())
            .await
            {
                Ok(t) => t,
                Err(e) => {
                    error!(category = %cat, error = %e, "Failed to list templates by category");
                    return error_result(&format!("Database error: {e}"));
                }
            }
        } else {
            match sqlx::query_as(
                "SELECT * FROM templates.templates ORDER BY name LIMIT $1 OFFSET $2",
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.pool())
            .await
            {
                Ok(t) => t,
                Err(e) => {
                    error!(error = %e, "Failed to list templates");
                    return error_result(&format!("Database error: {e}"));
                }
            }
        };

        json_result(&serde_json::json!({
            "templates": templates,
            "limit": limit,
            "offset": offset,
            "count": templates.len()
        }))
    }

    async fn handle_get_template(&self, id_or_name: &str) -> CallToolResult {
        let trimmed = id_or_name.trim();
        if trimmed.is_empty() {
            return error_result("Parameter 'template_id' must not be empty or whitespace-only");
        }
        match self.find_template(trimmed).await {
            Ok(Some(t)) => json_result(&t),
            Ok(None) => error_result(&format!("Template '{trimmed}' not found")),
            Err(e) => error_result(&e),
        }
    }

    async fn handle_update_template(&self, args: &serde_json::Value) -> CallToolResult {
        let template_id = match require_template_id(args) {
            Ok(t) => t,
            Err(e) => return e,
        };

        let existing = match self.find_template(&template_id).await {
            Ok(Some(t)) => t,
            Ok(None) => return error_result(&format!("Template '{template_id}' not found")),
            Err(e) => return error_result(&e),
        };

        let body = optional_trimmed_str(args, "body").unwrap_or(existing.body);
        let category = optional_trimmed_str(args, "category").unwrap_or(existing.category);
        let metadata = match validate_metadata(args, existing.metadata) {
            Ok(m) => m,
            Err(e) => return e,
        };

        // Validate body length
        if body.len() > MAX_BODY_LEN {
            return error_result(&format!(
                "Template body too long ({} chars). Maximum is {MAX_BODY_LEN}.",
                body.len()
            ));
        }

        // Validate category
        if let Err(e) = validate_category(&category) {
            return e;
        }

        let variables = extract_variables(&body);

        match sqlx::query_as::<_, Template>(
            "UPDATE templates.templates SET body = $1, category = $2, metadata = $3, variables = $4, updated_at = now() WHERE id = $5 RETURNING *",
        )
        .bind(&body)
        .bind(&category)
        .bind(&metadata)
        .bind(&variables)
        .bind(&existing.id)
        .fetch_one(self.db.pool())
        .await
        {
            Ok(template) => {
                info!(name = %template.name, id = %template.id, "Updated template");
                json_result(&template)
            }
            Err(e) => {
                error!(template_id = %existing.id, error = %e, "Failed to update template");
                error_result(&format!("Failed to update template: {e}"))
            }
        }
    }

    async fn handle_delete_template(&self, id_or_name: &str) -> CallToolResult {
        let trimmed = id_or_name.trim();
        if trimmed.is_empty() {
            return error_result("Parameter 'template_id' must not be empty or whitespace-only");
        }

        let template = match self.find_template(trimmed).await {
            Ok(Some(t)) => t,
            Ok(None) => return error_result(&format!("Template '{trimmed}' not found")),
            Err(e) => return error_result(&e),
        };

        match sqlx::query("DELETE FROM templates.templates WHERE id = $1")
            .bind(&template.id)
            .execute(self.db.pool())
            .await
        {
            Ok(_) => {
                info!(name = %template.name, id = %template.id, "Deleted template");
                json_result(&serde_json::json!({
                    "deleted": true,
                    "name": template.name,
                    "id": template.id
                }))
            }
            Err(e) => {
                error!(template_id = %template.id, error = %e, "Failed to delete template");
                error_result(&format!("Failed to delete template: {e}"))
            }
        }
    }

    async fn handle_clone_template(&self, args: &serde_json::Value) -> CallToolResult {
        let template_id = match require_template_id(args) {
            Ok(t) => t,
            Err(e) => return e,
        };
        let new_name = match require_trimmed_str(args, "new_name") {
            Ok(n) => n,
            Err(e) => return e,
        };

        // Validate new name length
        if new_name.len() > MAX_NAME_LEN {
            return error_result(&format!(
                "Template name too long ({} chars). Maximum is {MAX_NAME_LEN}.",
                new_name.len()
            ));
        }

        let source = match self.find_template(&template_id).await {
            Ok(Some(t)) => t,
            Ok(None) => return error_result(&format!("Template '{template_id}' not found")),
            Err(e) => return error_result(&e),
        };

        let new_id = uuid::Uuid::new_v4().to_string();

        match sqlx::query_as::<_, Template>(
            "INSERT INTO templates.templates (id, name, category, body, variables, metadata) VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
        )
        .bind(&new_id)
        .bind(&new_name)
        .bind(&source.category)
        .bind(&source.body)
        .bind(&source.variables)
        .bind(&source.metadata)
        .fetch_one(self.db.pool())
        .await
        {
            Ok(template) => {
                info!(
                    source = %source.name,
                    clone = %new_name,
                    new_id = %new_id,
                    "Cloned template"
                );
                json_result(&serde_json::json!({
                    "cloned_from": source.name,
                    "template": template
                }))
            }
            Err(e) => {
                error!(source = %source.name, new_name = %new_name, error = %e, "Failed to clone template");
                if e.to_string().contains("unique") || e.to_string().contains("duplicate") {
                    error_result(&format!("A template named '{new_name}' already exists"))
                } else {
                    error_result(&format!("Failed to clone template: {e}"))
                }
            }
        }
    }

    async fn handle_template_usage(&self, args: &serde_json::Value) -> CallToolResult {
        let template_id = match require_template_id(args) {
            Ok(t) => t,
            Err(e) => return e,
        };

        let limit = parse_limit(args, DEFAULT_LIMIT);
        let offset = parse_offset(args);

        let template = match self.find_template(&template_id).await {
            Ok(Some(t)) => t,
            Ok(None) => return error_result(&format!("Template '{template_id}' not found")),
            Err(e) => return error_result(&e),
        };

        let recent: Vec<UsageLogEntry> = match sqlx::query_as(
            "SELECT * FROM templates.usage_log WHERE template_id = $1 ORDER BY rendered_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(&template.id)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.db.pool())
        .await
        {
            Ok(entries) => entries,
            Err(e) => {
                error!(template_id = %template.id, error = %e, "Failed to fetch usage log");
                return error_result(&format!("Failed to fetch usage log: {e}"));
            }
        };

        json_result(&serde_json::json!({
            "template_id": template.id,
            "template_name": template.name,
            "usage_count": template.usage_count,
            "recent_usage": recent,
            "limit": limit,
            "offset": offset
        }))
    }

    async fn handle_search_templates(&self, args: &serde_json::Value) -> CallToolResult {
        let query = match require_trimmed_str(args, "query") {
            Ok(q) => q,
            Err(e) => return e,
        };
        if query.len() > MAX_QUERY_LEN {
            return error_result(&format!(
                "Search query too long ({} chars). Maximum is {MAX_QUERY_LEN}.",
                query.len()
            ));
        }

        let category = optional_trimmed_str(args, "category");
        let limit = parse_limit(args, DEFAULT_LIMIT);
        let offset = parse_offset(args);

        if let Some(ref cat) = category {
            if let Err(e) = validate_category(cat) {
                return e;
            }
        }

        let pattern = format!("%{query}%");

        let templates: Vec<Template> = if let Some(cat) = &category {
            match sqlx::query_as(
                "SELECT * FROM templates.templates WHERE (name ILIKE $1 OR body ILIKE $1) AND category = $2 ORDER BY usage_count DESC, name LIMIT $3 OFFSET $4",
            )
            .bind(&pattern)
            .bind(cat)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.pool())
            .await
            {
                Ok(t) => t,
                Err(e) => {
                    error!(query = %query, category = %cat, error = %e, "Failed to search templates");
                    return error_result(&format!("Database error: {e}"));
                }
            }
        } else {
            match sqlx::query_as(
                "SELECT * FROM templates.templates WHERE name ILIKE $1 OR body ILIKE $1 ORDER BY usage_count DESC, name LIMIT $2 OFFSET $3",
            )
            .bind(&pattern)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.pool())
            .await
            {
                Ok(t) => t,
                Err(e) => {
                    error!(query = %query, error = %e, "Failed to search templates");
                    return error_result(&format!("Database error: {e}"));
                }
            }
        };

        info!(query = %query, results = templates.len(), "Searched templates");
        json_result(&serde_json::json!({
            "query": query,
            "templates": templates,
            "limit": limit,
            "offset": offset,
            "count": templates.len()
        }))
    }
}

// ============================================================================
// ServerHandler trait implementation
// ============================================================================

impl ServerHandler for TemplatesMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "DataXLR8 Templates MCP — manage templates with {{variable}} placeholders, render, clone, and track usage"
                    .into(),
            ),
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, rmcp::ErrorData>> + Send + '_
    {
        async {
            Ok(ListToolsResult {
                tools: build_tools(),
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, rmcp::ErrorData>> + Send + '_
    {
        async move {
            let args =
                serde_json::to_value(&request.arguments).unwrap_or(serde_json::Value::Null);
            let name_str: &str = request.name.as_ref();

            let result = match name_str {
                "create_template" => self.handle_create_template(&args).await,
                "render_template" => self.handle_render_template(&args).await,
                "list_templates" => self.handle_list_templates(&args).await,
                "get_template" => {
                    match require_template_id(&args) {
                        Ok(id) => self.handle_get_template(&id).await,
                        Err(e) => e,
                    }
                }
                "update_template" => self.handle_update_template(&args).await,
                "delete_template" => {
                    match require_template_id(&args) {
                        Ok(id) => self.handle_delete_template(&id).await,
                        Err(e) => e,
                    }
                }
                "clone_template" => self.handle_clone_template(&args).await,
                "template_usage" => self.handle_template_usage(&args).await,
                "search_templates" => self.handle_search_templates(&args).await,
                _ => {
                    warn!(tool = name_str, "Unknown tool called");
                    error_result(&format!("Unknown tool: {}", request.name))
                }
            };

            Ok(result)
        }
    }
}
