use dataxlr8_mcp_core::mcp::{empty_schema, error_result, get_str, json_result, make_schema};
use dataxlr8_mcp_core::Database;
use regex::Regex;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::ServerHandler;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

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

#[derive(Debug, Serialize)]
pub struct TemplateUsageReport {
    pub template_id: String,
    pub template_name: String,
    pub usage_count: i32,
    pub recent_usage: Vec<UsageLogEntry>,
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
            description: Some("List all templates, optionally filtered by category".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "category": { "type": "string", "enum": ["email", "proposal", "invoice", "report"], "description": "Filter by category" }
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
                    "limit": { "type": "integer", "description": "Max recent usage entries to return (default: 20)" }
                }),
                vec!["template_id"],
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
        // Try by ID first, then by name
        match sqlx::query_as::<_, Template>(
            "SELECT * FROM templates.templates WHERE id = $1 OR name = $1",
        )
        .bind(id_or_name)
        .fetch_optional(self.db.pool())
        .await
        {
            Ok(t) => Ok(t),
            Err(e) => Err(format!("Database error: {e}")),
        }
    }

    // ---- Tool handlers ----

    async fn handle_create_template(&self, args: &serde_json::Value) -> CallToolResult {
        let name = match get_str(args, "name") {
            Some(n) => n,
            None => return error_result("Missing required parameter: name"),
        };
        let body = match get_str(args, "body") {
            Some(b) => b,
            None => return error_result("Missing required parameter: body"),
        };
        let category = get_str(args, "category").unwrap_or_else(|| "email".into());

        if !["email", "proposal", "invoice", "report"].contains(&category.as_str()) {
            return error_result("category must be one of: email, proposal, invoice, report");
        }

        let metadata = args
            .get("metadata")
            .cloned()
            .unwrap_or(serde_json::json!({}));

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
                info!(name = name, category = category, "Created template");
                json_result(&template)
            }
            Err(e) => error_result(&format!("Failed to create template: {e}")),
        }
    }

    async fn handle_render_template(&self, args: &serde_json::Value) -> CallToolResult {
        let template_id = match get_str(args, "template_id") {
            Some(t) => t,
            None => return error_result("Missing required parameter: template_id"),
        };

        let variables = match args.get("variables") {
            Some(v) if v.is_object() => v.clone(),
            _ => return error_result("Missing required parameter: variables (must be an object)"),
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
            error!(template_id = template.id, error = %e, "Failed to log usage");
        }

        // Increment usage_count
        if let Err(e) = sqlx::query(
            "UPDATE templates.templates SET usage_count = usage_count + 1, updated_at = now() WHERE id = $1",
        )
        .bind(&template.id)
        .execute(self.db.pool())
        .await
        {
            error!(template_id = template.id, error = %e, "Failed to increment usage count");
        }

        info!(template = template.name, "Rendered template");
        json_result(&serde_json::json!({
            "template_name": template.name,
            "rendered": rendered,
            "variables_used": variables
        }))
    }

    async fn handle_list_templates(&self, args: &serde_json::Value) -> CallToolResult {
        let category = get_str(args, "category");

        let templates: Vec<Template> = if let Some(cat) = &category {
            match sqlx::query_as(
                "SELECT * FROM templates.templates WHERE category = $1 ORDER BY name",
            )
            .bind(cat)
            .fetch_all(self.db.pool())
            .await
            {
                Ok(t) => t,
                Err(e) => return error_result(&format!("Database error: {e}")),
            }
        } else {
            match sqlx::query_as("SELECT * FROM templates.templates ORDER BY name")
                .fetch_all(self.db.pool())
                .await
            {
                Ok(t) => t,
                Err(e) => return error_result(&format!("Database error: {e}")),
            }
        };

        json_result(&templates)
    }

    async fn handle_get_template(&self, id_or_name: &str) -> CallToolResult {
        match self.find_template(id_or_name).await {
            Ok(Some(t)) => json_result(&t),
            Ok(None) => error_result(&format!("Template '{id_or_name}' not found")),
            Err(e) => error_result(&e),
        }
    }

    async fn handle_update_template(&self, args: &serde_json::Value) -> CallToolResult {
        let template_id = match get_str(args, "template_id") {
            Some(t) => t,
            None => return error_result("Missing required parameter: template_id"),
        };

        let existing = match self.find_template(&template_id).await {
            Ok(Some(t)) => t,
            Ok(None) => return error_result(&format!("Template '{template_id}' not found")),
            Err(e) => return error_result(&e),
        };

        let body = get_str(args, "body").unwrap_or(existing.body);
        let category = get_str(args, "category").unwrap_or(existing.category);
        let metadata = args
            .get("metadata")
            .cloned()
            .unwrap_or(existing.metadata);

        if !["email", "proposal", "invoice", "report"].contains(&category.as_str()) {
            return error_result("category must be one of: email, proposal, invoice, report");
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
                info!(name = template.name, "Updated template");
                json_result(&template)
            }
            Err(e) => error_result(&format!("Failed to update template: {e}")),
        }
    }

    async fn handle_delete_template(&self, id_or_name: &str) -> CallToolResult {
        // Find first to get the actual ID (CASCADE handles usage_log)
        let template = match self.find_template(id_or_name).await {
            Ok(Some(t)) => t,
            Ok(None) => return error_result(&format!("Template '{id_or_name}' not found")),
            Err(e) => return error_result(&e),
        };

        match sqlx::query("DELETE FROM templates.templates WHERE id = $1")
            .bind(&template.id)
            .execute(self.db.pool())
            .await
        {
            Ok(_) => {
                info!(name = template.name, "Deleted template");
                json_result(&serde_json::json!({
                    "deleted": true,
                    "name": template.name,
                    "id": template.id
                }))
            }
            Err(e) => error_result(&format!("Failed to delete template: {e}")),
        }
    }

    async fn handle_clone_template(&self, args: &serde_json::Value) -> CallToolResult {
        let template_id = match get_str(args, "template_id") {
            Some(t) => t,
            None => return error_result("Missing required parameter: template_id"),
        };
        let new_name = match get_str(args, "new_name") {
            Some(n) => n,
            None => return error_result("Missing required parameter: new_name"),
        };

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
                    source = source.name,
                    clone = new_name,
                    "Cloned template"
                );
                json_result(&serde_json::json!({
                    "cloned_from": source.name,
                    "template": template
                }))
            }
            Err(e) => error_result(&format!("Failed to clone template: {e}")),
        }
    }

    async fn handle_template_usage(&self, args: &serde_json::Value) -> CallToolResult {
        let template_id = match get_str(args, "template_id") {
            Some(t) => t,
            None => return error_result("Missing required parameter: template_id"),
        };

        let limit = args
            .get("limit")
            .and_then(|v| v.as_i64())
            .unwrap_or(20)
            .min(100) as i32;

        let template = match self.find_template(&template_id).await {
            Ok(Some(t)) => t,
            Ok(None) => return error_result(&format!("Template '{template_id}' not found")),
            Err(e) => return error_result(&e),
        };

        let recent: Vec<UsageLogEntry> = match sqlx::query_as(
            "SELECT * FROM templates.usage_log WHERE template_id = $1 ORDER BY rendered_at DESC LIMIT $2",
        )
        .bind(&template.id)
        .bind(limit)
        .fetch_all(self.db.pool())
        .await
        {
            Ok(entries) => entries,
            Err(e) => {
                error!(template_id = template.id, error = %e, "Failed to fetch usage log");
                Vec::new()
            }
        };

        json_result(&TemplateUsageReport {
            template_id: template.id,
            template_name: template.name,
            usage_count: template.usage_count,
            recent_usage: recent,
        })
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
                "get_template" => match get_str(&args, "template_id") {
                    Some(id) => self.handle_get_template(&id).await,
                    None => error_result("Missing required parameter: template_id"),
                },
                "update_template" => self.handle_update_template(&args).await,
                "delete_template" => match get_str(&args, "template_id") {
                    Some(id) => self.handle_delete_template(&id).await,
                    None => error_result("Missing required parameter: template_id"),
                },
                "clone_template" => self.handle_clone_template(&args).await,
                "template_usage" => self.handle_template_usage(&args).await,
                _ => error_result(&format!("Unknown tool: {}", request.name)),
            };

            Ok(result)
        }
    }
}
