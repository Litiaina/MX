use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileAttachment {
    pub uid: String,
    pub file_name: String,
    pub mime_type: String,
    pub size: u64,

    // Stable N1 object key. New dynamic records use:
    // records/<record_uid>/<attachment_uid>__<file_name>
    pub object_key: String,

    pub version_id: Option<String>,
}
