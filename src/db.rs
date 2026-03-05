use anyhow::Result;
use sqlx::PgPool;

pub async fn setup_schema(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(
        r#"
        CREATE SCHEMA IF NOT EXISTS templates;

        CREATE TABLE IF NOT EXISTS templates.templates (
            id           TEXT PRIMARY KEY,
            name         TEXT NOT NULL UNIQUE,
            category     TEXT NOT NULL DEFAULT 'email'
                         CHECK (category IN ('email', 'proposal', 'invoice', 'report')),
            body         TEXT NOT NULL DEFAULT '',
            variables    TEXT[] NOT NULL DEFAULT '{}',
            metadata     JSONB NOT NULL DEFAULT '{}',
            usage_count  INTEGER NOT NULL DEFAULT 0,
            created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE TABLE IF NOT EXISTS templates.usage_log (
            id             TEXT PRIMARY KEY,
            template_id    TEXT NOT NULL REFERENCES templates.templates(id) ON DELETE CASCADE,
            rendered_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
            variables_used JSONB NOT NULL DEFAULT '{}'
        );

        CREATE INDEX IF NOT EXISTS idx_templates_name ON templates.templates(name);
        CREATE INDEX IF NOT EXISTS idx_templates_category ON templates.templates(category);
        CREATE INDEX IF NOT EXISTS idx_usage_log_template_id ON templates.usage_log(template_id);
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}
