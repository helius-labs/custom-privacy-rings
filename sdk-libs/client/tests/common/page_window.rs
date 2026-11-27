//! An honest indexer's paging over a full answer.

use spr_client::PaginationToken;
use spr_indexer_api::{Page, Positioned};

pub struct MockPageWindow {
    pub token: Option<PaginationToken>,
    pub limit: usize,
}

/// The transactions after `token`, at most `limit`, a token only ahead of more.
pub fn page_window<T: Positioned>(mut page: Page<T>, window: MockPageWindow) -> Page<T> {
    page.data
        .retain(|row| window.token.is_none_or(|token| row.position() > token));
    let more = page.data.len() > window.limit;
    page.data.truncate(window.limit);
    page.pagination_token = page.data.last().filter(|_| more).map(|row| row.position());
    page
}
