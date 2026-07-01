//! The arcanum Postgres store: per-agent token-monitor state, plus the
//! `arcanum_monitor` NOTIFY channel the daemon LISTENs on.
//!
//! Connected lazily from [`Context`](crate::context::Context) (only the daemon,
//! `mcp arcanum begin`, and `load_skill` touch it). Runtime queries only — no
//! compile-time macros, so no `DATABASE_URL` is needed at build time.

use sqlx::postgres::{PgListener, PgPool, PgPoolOptions};

/// The embedded schema, applied idempotently on [`Db::connect`].
const SCHEMA: &str = include_str!("schema.sql");

/// The Postgres NOTIFY channel `mcp arcanum begin` pings and the daemon LISTENs.
pub const MONITOR_CHANNEL: &str = "arcanum_monitor";

/// Cloneable handle to the Postgres pool. Clone is cheap (the pool is `Arc`).
#[derive(Clone)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    /// Open the pool and apply the schema. The schema is `CREATE TABLE IF NOT
    /// EXISTS`, so this is idempotent.
    pub async fn connect(url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .connect(url)
            .await?;
        sqlx::raw_sql(SCHEMA).execute(&pool).await?;
        Ok(Self { pool })
    }

    /// Record (or refresh) an agent's `token_repeat`, creating the row if new.
    pub async fn upsert_token_repeat(&self, aih: &str, token_repeat: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO arcanum_agents (agent_instance_hierarchy, token_repeat) VALUES ($1, $2) \
             ON CONFLICT (agent_instance_hierarchy) DO UPDATE SET token_repeat = excluded.token_repeat",
        )
        .bind(aih)
        .bind(token_repeat)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The agent's `token_repeat`, or `None` if the row is absent.
    pub async fn token_repeat(&self, aih: &str) -> Result<Option<i64>, sqlx::Error> {
        sqlx::query_scalar("SELECT token_repeat FROM arcanum_agents WHERE agent_instance_hierarchy = $1")
            .bind(aih)
            .fetch_optional(&self.pool)
            .await
    }

    /// The injection baseline, or `None` if the row is absent OR the column is NULL.
    pub async fn last_total_tokens(&self, aih: &str) -> Result<Option<i64>, sqlx::Error> {
        let row: Option<Option<i64>> = sqlx::query_scalar(
            "SELECT last_total_tokens FROM arcanum_agents WHERE agent_instance_hierarchy = $1",
        )
        .bind(aih)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.flatten())
    }

    /// Advance the injection baseline (the row must already exist).
    pub async fn set_last_total_tokens(&self, aih: &str, value: i64) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE arcanum_agents SET last_total_tokens = $2 WHERE agent_instance_hierarchy = $1")
            .bind(aih)
            .bind(value)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The currently loaded skill content, or `None` if absent/unloaded.
    pub async fn skill_content(&self, aih: &str) -> Result<Option<String>, sqlx::Error> {
        let row: Option<Option<String>> = sqlx::query_scalar(
            "SELECT skill_content FROM arcanum_agents WHERE agent_instance_hierarchy = $1",
        )
        .bind(aih)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.flatten())
    }

    /// Upsert the loaded skill content, creating the row (with `token_repeat`) if new.
    pub async fn set_skill_content(
        &self,
        aih: &str,
        token_repeat: i64,
        content: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO arcanum_agents (agent_instance_hierarchy, token_repeat, skill_content) \
             VALUES ($1, $2, $3) \
             ON CONFLICT (agent_instance_hierarchy) \
             DO UPDATE SET token_repeat = excluded.token_repeat, skill_content = excluded.skill_content",
        )
        .bind(aih)
        .bind(token_repeat)
        .bind(content)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Drop an agent's row (the agent instance went inactive).
    pub async fn delete(&self, aih: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM arcanum_agents WHERE agent_instance_hierarchy = $1")
            .bind(aih)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// NOTIFY the daemon that `aih` should be (re)evaluated for monitoring.
    pub async fn notify_monitor(&self, aih: &str) -> Result<(), sqlx::Error> {
        sqlx::query("SELECT pg_notify($1, $2)")
            .bind(MONITOR_CHANNEL)
            .bind(aih)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// A dedicated listener on the [`MONITOR_CHANNEL`].
    pub async fn monitor_listener(&self) -> Result<PgListener, sqlx::Error> {
        let mut listener = PgListener::connect_with(&self.pool).await?;
        listener.listen(MONITOR_CHANNEL).await?;
        Ok(listener)
    }
}
