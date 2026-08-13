use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::modules::search::domain::repositories::SearchHit;

#[derive(Debug, Default, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchQuery {
    /// What to look for. Matched mid-string and case-insensitively, the same way
    /// every list screen's own search filter matches.
    pub q: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SearchResults {
    /// Echoed back so a client can discard a response that arrived after the
    /// user typed on — a dropdown that redraws per keystroke gets them out of
    /// order eventually.
    pub query: String,
    /// Flat and grouped by the client. The server has no view on how results
    /// should be laid out, and one list keeps the payload obvious.
    pub hits: Vec<SearchHit>,
}
