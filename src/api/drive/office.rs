//! Stateless Office-open snapshot. Saves reuse Drive's immutable multipart versions.
use super::{Error, database, store};
use crate::middleware::auth::Claims;
use axum::{Json, extract::Path};
use rusqlite::TransactionBehavior;
use serde_json::{Value, json};

pub(super) async fn open(claims: Claims, Path(uid): Path<String>) -> Result<Json<Value>, Error> {
    Ok(Json(
        database(move |c| {
            let tx = rusqlite::Transaction::new_unchecked(c, TransactionBehavior::Deferred)?;
            let item = store::accessible(&tx, &uid, &claims.uid, false)?;
            if item.kind != "file" || !supported(&item.original_file_name) {
                return Err(store::problem(
                    400,
                    "This file format is not supported by MX Office.",
                ));
            }
            let version_uid: String = tx.query_row(
                "SELECT current_version_uid FROM mx_drive_items WHERE uid=?1",
                [&uid],
                |row| row.get(0),
            )?;
            tx.commit()?;
            let editing_supported = !item.original_file_name.rsplit_once('.').is_some_and(|(_, extension)| {
                matches!(extension.to_ascii_lowercase().as_str(), "ppt" | "pptx" | "odp")
            });
            Ok(json!({"item": item, "version_uid": version_uid, "editing_supported": editing_supported}))
        })
        .await?,
    ))
}

fn supported(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        matches!(
            extension.to_ascii_lowercase().as_str(),
            "doc" | "docx" | "odt" | "rtf" | "xls" | "xlsx" | "ods" | "ppt" | "pptx" | "odp"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formats_are_explicit_and_macro_enabled_files_are_not_editable() {
        assert!(supported("Budget.XLSX"));
        assert!(supported("letter.odt"));
        assert!(!supported("script.xlsm"));
        assert!(!supported("document.docx.exe"));
    }
}
