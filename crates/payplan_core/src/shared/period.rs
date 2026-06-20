use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Period {
    pub starts_at: DateTime<Utc>,
    pub ends_at: Option<DateTime<Utc>>,
}
