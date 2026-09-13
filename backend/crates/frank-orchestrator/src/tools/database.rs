//! Database connector tool domain.

use std::path::PathBuf;

use base64::Engine;
use frank_protocol::*;
use futures_util::StreamExt;
use serde_json::{Value, json};
use sqlx::{
    Column, Row, postgres::PgPoolOptions, postgres::PgRow, sqlite::SqliteConnectOptions,
    sqlite::SqlitePoolOptions, sqlite::SqliteRow,
};

use super::{CONNECTOR_TIMEOUT, ToolExecutionContext, required_string};
use crate::{
    DatabaseStatementClass, classify_database_statement, organization_tool_profile,
    validate_sqlite_path,
};

pub(crate) async fn dispatch(
    context: &ToolExecutionContext<'_>,
    name: &str,
    input: &Value,
) -> Result<Option<Value>, String> {
    let statement = required_string(input, "statement")?;
    let class = classify_database_statement(&statement);
    if class == DatabaseStatementClass::Reject {
        return Err("database statement is ambiguous or unsafe and was rejected".into());
    }
    if matches!(name, "database_inspect" | "database_read")
        && class != DatabaseStatementClass::ReadOnly
    {
        return Err("database inspect/read accepts only a single read-only statement".into());
    }
    let profile = organization_tool_profile(context.snapshot, context.agent_id, name)
        .ok_or_else(|| "Organization connector profile is unavailable".to_string())?;
    let result = match profile.kind {
        ConnectorKind::Sqlite => {
            execute_sqlite_database(&profile, context.snapshot, name, &statement).await?
        }
        ConnectorKind::Postgres => {
            let secret = context
                .orchestrator
                .connector_secret(profile.id)
                .await
                .map_err(|error| format!("PostgreSQL credential lookup failed: {error}"))?
                .ok_or_else(|| "PostgreSQL connector credential is not configured".to_string())?;
            execute_postgres_database(&profile, &secret, name, &statement).await?
        }
        _ => return Err("connector profile kind does not support database tools".into()),
    };
    Ok(Some(result))
}

pub(crate) async fn execute_sqlite_database(
    profile: &ConnectorProfileView,
    snapshot: &Snapshot,
    tool: &str,
    statement: &str,
) -> Result<Value, String> {
    let path = profile
        .config
        .get("path")
        .or_else(|| profile.config.get("database_path"))
        .and_then(Value::as_str)
        .ok_or_else(|| "SQLite connector profile is missing its non-secret path".to_string())?;
    let allowed_roots = snapshot
        .server
        .allowed_project_roots
        .iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let path = validate_sqlite_path(path, &allowed_roots).map_err(|error| error.to_string())?;
    let class = classify_database_statement(statement);
    if class == DatabaseStatementClass::Reject {
        return Err("database statement is ambiguous or unsafe and was rejected".into());
    }
    if matches!(tool, "database_inspect" | "database_read")
        && class != DatabaseStatementClass::ReadOnly
    {
        return Err("database inspect/read accepts only a read-only statement".into());
    }
    if tool == "database_write" && class != DatabaseStatementClass::Write {
        return Err("database write requires a write statement".into());
    }
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(false)
        .read_only(class == DatabaseStatementClass::ReadOnly);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|error| format!("SQLite connector could not open the database: {error}"))?;
    if class == DatabaseStatementClass::Write {
        let mut transaction = pool
            .begin()
            .await
            .map_err(|error| format!("SQLite transaction could not start: {error}"))?;
        let result = sqlx::query(statement)
            .execute(&mut *transaction)
            .await
            .map_err(|error| format!("SQLite write failed: {error}"))?;
        transaction
            .commit()
            .await
            .map_err(|error| format!("SQLite transaction could not commit: {error}"))?;
        pool.close().await;
        return Ok(json!({
            "profile_id": profile.id.to_string(),
            "affected_rows": result.rows_affected(),
        }));
    }
    let mut rows = sqlx::query(statement).fetch(&pool);
    let mut values = Vec::with_capacity(256);
    let mut truncated = false;
    while let Some(row) = rows.next().await {
        let row = row.map_err(|error| format!("SQLite read failed: {error}"))?;
        if values.len() == 256 {
            truncated = true;
            break;
        }
        values.push(sqlite_row_to_json(&row));
    }
    drop(rows);
    pool.close().await;
    Ok(json!({
        "profile_id": profile.id.to_string(),
        "rows": values,
        "truncated": truncated,
    }))
}

pub(crate) async fn execute_postgres_database(
    profile: &ConnectorProfileView,
    secret: &str,
    tool: &str,
    statement: &str,
) -> Result<Value, String> {
    let class = classify_database_statement(statement);
    if class == DatabaseStatementClass::Reject {
        return Err("database statement is ambiguous or unsafe and was rejected".into());
    }
    if matches!(tool, "database_inspect" | "database_read")
        && class != DatabaseStatementClass::ReadOnly
    {
        return Err("database inspect/read accepts only a single read-only statement".into());
    }
    if tool == "database_write" && class != DatabaseStatementClass::Write {
        return Err("database write requires a write statement".into());
    }
    let dsn = serde_json::from_str::<Value>(secret)
        .ok()
        .and_then(|value| value.get("dsn").and_then(Value::as_str).map(str::to_owned))
        .filter(|dsn| !dsn.trim().is_empty())
        .unwrap_or_else(|| secret.trim().to_owned());
    if !dsn.starts_with("postgres://") && !dsn.starts_with("postgresql://") {
        return Err("PostgreSQL credential must be a postgres DSN".into());
    }
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(CONNECTOR_TIMEOUT)
        .connect(&dsn)
        .await
        .map_err(|error| sanitize_database_error(&error.to_string(), &dsn))?;
    if class == DatabaseStatementClass::Write {
        let mut transaction = pool
            .begin()
            .await
            .map_err(|error| format!("PostgreSQL transaction could not start: {error}"))?;
        let result = sqlx::query(statement)
            .execute(&mut *transaction)
            .await
            .map_err(|error| sanitize_database_error(&error.to_string(), &dsn))?;
        transaction
            .commit()
            .await
            .map_err(|error| format!("PostgreSQL transaction could not commit: {error}"))?;
        pool.close().await;
        return Ok(json!({
            "profile_id": profile.id.to_string(),
            "affected_rows": result.rows_affected(),
        }));
    }
    let mut rows = sqlx::query(statement).fetch(&pool);
    let mut values = Vec::with_capacity(256);
    let mut truncated = false;
    while let Some(row) = rows.next().await {
        let row = row.map_err(|error| sanitize_database_error(&error.to_string(), &dsn))?;
        if values.len() == 256 {
            truncated = true;
            break;
        }
        values.push(postgres_row_to_json(&row));
    }
    drop(rows);
    pool.close().await;
    Ok(json!({
        "profile_id": profile.id.to_string(),
        "rows": values,
        "truncated": truncated,
    }))
}

fn sqlite_row_to_json(row: &SqliteRow) -> Value {
    row_to_json(row)
}

fn postgres_row_to_json(row: &PgRow) -> Value {
    row_to_json(row)
}

fn row_to_json<R>(row: &R) -> Value
where
    R: Row,
    for<'r> String: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    for<'r> i64: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    for<'r> f64: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    for<'r> Vec<u8>: sqlx::Decode<'r, R::Database> + sqlx::Type<R::Database>,
    usize: sqlx::ColumnIndex<R>,
{
    let mut object = serde_json::Map::new();
    for (index, column) in row.columns().iter().enumerate() {
        let value = row
            .try_get::<String, _>(index)
            .map(Value::String)
            .or_else(|_| row.try_get::<i64, _>(index).map(|value| json!(value)))
            .or_else(|_| row.try_get::<f64, _>(index).map(|value| json!(value)))
            .or_else(|_| {
                row.try_get::<Vec<u8>, _>(index).map(|value| {
                    json!({"base64": base64::engine::general_purpose::STANDARD.encode(value)})
                })
            })
            .unwrap_or(Value::Null);
        object.insert(column.name().to_string(), value);
    }
    Value::Object(object)
}

fn sanitize_database_error(error: &str, secret: &str) -> String {
    error
        .replace(secret, "[redacted]")
        .chars()
        .take(512)
        .collect()
}
