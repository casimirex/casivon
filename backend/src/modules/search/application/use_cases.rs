use crate::error::AppResult;
use crate::modules::search::application::dto::{SearchQuery, SearchResults};
use crate::modules::search::domain::repositories::{SearchKind, SearchRepository};
use crate::shared::auth::CurrentUser;

/// Shorter than this and a search is not a search.
///
/// `ILIKE '%a%'` is a sequential scan over fifteen tables to return everything
/// that contains the letter a — expensive, and useless to whoever typed it.
const MINIMUM_TERM_LENGTH: usize = 2;

/// At most this many of any one kind, so a catalogue of two hundred matching
/// products cannot bury the single matching invoice.
const HITS_PER_KIND: i64 = 5;

pub struct SearchUseCases<R: SearchRepository> {
    repo: R,
}

impl<R: SearchRepository> SearchUseCases<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }

    pub async fn search(&self, query: SearchQuery, user: &CurrentUser) -> AppResult<SearchResults> {
        let term = query.q.unwrap_or_default().trim().to_string();

        if term.chars().count() < MINIMUM_TERM_LENGTH {
            return Ok(SearchResults { query: term, hits: Vec::new() });
        }

        // Kinds the caller may not see are left out of the query entirely rather
        // than filtered from its results. Nothing then depends on remembering a
        // second check, and the database never reads rows this user could not
        // have been shown.
        let kinds: Vec<SearchKind> = SearchKind::ALL
            .into_iter()
            .filter(|kind| match kind.required_roles() {
                Some(roles) => user.require_any_role(roles).is_ok(),
                None => true,
            })
            .collect();

        let hits = self.repo.search(&term, &kinds, HITS_PER_KIND).await?;
        Ok(SearchResults { query: term, hits })
    }
}
