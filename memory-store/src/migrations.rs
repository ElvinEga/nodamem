//! Embedded SQL migration runner for the Nodamem local-first store.

use libsql::{params, Connection};

use crate::error::StoreError;

#[derive(Debug, Clone, Copy)]
struct Migration {
    version: &'static str,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: "0000",
        name: "schema_migrations",
        sql: include_str!("../migrations/0000_schema_migrations.sql"),
    },
    Migration {
        version: "0001",
        name: "nodes",
        sql: include_str!("../migrations/0001_nodes.sql"),
    },
    Migration {
        version: "0002",
        name: "edges",
        sql: include_str!("../migrations/0002_edges.sql"),
    },
    Migration {
        version: "0003",
        name: "node_sources",
        sql: include_str!("../migrations/0003_node_sources.sql"),
    },
    Migration {
        version: "0004",
        name: "lessons",
        sql: include_str!("../migrations/0004_lessons.sql"),
    },
    Migration {
        version: "0005",
        name: "lesson_sources",
        sql: include_str!("../migrations/0005_lesson_sources.sql"),
    },
    Migration {
        version: "0006",
        name: "trait_state",
        sql: include_str!("../migrations/0006_trait_state.sql"),
    },
    Migration {
        version: "0007",
        name: "trait_events",
        sql: include_str!("../migrations/0007_trait_events.sql"),
    },
    Migration {
        version: "0008",
        name: "checkpoints",
        sql: include_str!("../migrations/0008_checkpoints.sql"),
    },
    Migration {
        version: "0009",
        name: "imagined_nodes",
        sql: include_str!("../migrations/0009_imagined_nodes.sql"),
    },
    Migration {
        version: "0010",
        name: "working_memory",
        sql: include_str!("../migrations/0010_working_memory.sql"),
    },
    Migration {
        version: "0011",
        name: "node_embeddings",
        sql: include_str!("../migrations/0011_node_embeddings.sql"),
    },
    Migration {
        version: "0012",
        name: "node_recall_stats",
        sql: include_str!("../migrations/0012_node_recall_stats.sql"),
    },
    Migration {
        version: "0013",
        name: "self_model_snapshots",
        sql: include_str!("../migrations/0013_self_model_snapshots.sql"),
    },
    Migration {
        version: "0014",
        name: "trait_events_audit_fields",
        sql: include_str!("../migrations/0014_trait_events_audit_fields.sql"),
    },
];

pub async fn run_migrations(connection: &Connection) -> Result<(), StoreError> {
    for migration in MIGRATIONS {
        let should_apply = migration.version == "0000"
            || !migration_applied(connection, migration.version).await?;

        if should_apply {
            connection.execute_batch(migration.sql).await?;

            if migration.version != "0000" {
                record_migration(connection, migration).await?;
            }
        }
    }

    Ok(())
}

async fn migration_applied(connection: &Connection, version: &str) -> Result<bool, StoreError> {
    let mut rows = connection
        .query(
            "SELECT version FROM _nodamem_migrations WHERE version = ?1 LIMIT 1",
            params![version],
        )
        .await?;

    Ok(rows.next().await?.is_some())
}

async fn record_migration(
    connection: &Connection,
    migration: &Migration,
) -> Result<(), StoreError> {
    connection
        .execute(
            "INSERT INTO _nodamem_migrations (version, name) VALUES (?1, ?2)",
            params![migration.version, migration.name],
        )
        .await?;

    Ok(())
}
