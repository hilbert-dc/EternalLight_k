use rusqlite::Connection;

use crate::model_sources::{ModelSourceScope, validated_source};
use crate::{ProjectId, RepositoryError};

/// Proposition presence comes from validated source declarations, never a derived index.
pub(crate) fn proposition_source_visible(
    connection: &Connection,
    project_id: &ProjectId,
    scope: ModelSourceScope<'_>,
    batch_id: &str,
    proposition_id: &str,
) -> Result<bool, RepositoryError> {
    Ok(
        validated_source(connection, project_id, scope, batch_id)?.is_some_and(|source| {
            source
                .entities
                .iter()
                .any(|entity| entity["id"] == proposition_id && entity["kind"] == "proposition")
        }),
    )
}

#[cfg(test)]
#[path = "proposition_sources_tests.rs"]
mod tests;
