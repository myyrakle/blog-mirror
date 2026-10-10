pub mod category_repo;
pub mod cursor_repo;
pub mod job_repo;
pub mod post_repo;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

use crate::error::Result;

pub async fn create_pool(database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;
    Ok(pool)
}

pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    repair_migrations_table(pool).await?;
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

/// Restores `_sqlx_migrations.installed_on`'s default if it has gone missing.
///
/// sqlx creates that column as `TIMESTAMPTZ NOT NULL DEFAULT now()` and then
/// omits it from its INSERT, relying on the default. A database that lost the
/// default — ours did, most likely through a dump/restore that also reset the
/// serial sequences (#9) — fails every migration with:
///
/// ```text
/// null value in column "installed_on" of relation "_sqlx_migrations"
/// violates not-null constraint
/// ```
///
/// This cannot be repaired by a migration: the migrator's own bookkeeping
/// INSERT is what fails, so it never gets far enough to run one. It has to
/// happen before the migrator does anything, which is why it lives here.
async fn repair_migrations_table(pool: &PgPool) -> Result<()> {
    let needs_repair: Option<(bool,)> = sqlx::query_as(
        "SELECT column_default IS NULL
         FROM information_schema.columns
         WHERE table_schema = current_schema()
           AND table_name = '_sqlx_migrations'
           AND column_name = 'installed_on'",
    )
    .fetch_optional(pool)
    .await?;

    // No row means the table does not exist yet; sqlx will create it correctly.
    if needs_repair == Some((true,)) {
        tracing::warn!(
            "_sqlx_migrations.installed_on has no default; restoring it so migrations can run"
        );
        sqlx::query("ALTER TABLE _sqlx_migrations ALTER COLUMN installed_on SET DEFAULT now()")
            .execute(pool)
            .await?;
    }

    Ok(())
}
