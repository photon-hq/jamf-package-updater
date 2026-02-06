use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct PolicyListResponse {
    pub policies: Option<Vec<PolicySummary>>,
}

#[derive(Debug, Deserialize)]
pub struct PolicySummary {
    pub id: i64,
    pub name: String,
}

/// A policy that references the package we're updating.
#[derive(Debug)]
pub struct AffectedPolicy {
    pub id: i64,
    pub name: String,
}
