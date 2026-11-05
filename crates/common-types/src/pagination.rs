use serde::{Deserialize, Serialize};

pub const DEFAULT_PER_PAGE: u32 = 20;
pub const MAX_PER_PAGE: u32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(from = "RawPageRequest")]
pub struct PageRequest {
    page: u32,
    per_page: u32,
}

impl PageRequest {
    #[must_use]
    pub fn new(page: u32, per_page: u32) -> Self {
        Self {
            page: page.max(1),
            per_page: per_page.clamp(1, MAX_PER_PAGE),
        }
    }

    #[must_use]
    pub const fn page(&self) -> u32 {
        self.page
    }

    #[must_use]
    pub const fn per_page(&self) -> u32 {
        self.per_page
    }

    #[must_use]
    pub fn limit(&self) -> i64 {
        i64::from(self.per_page)
    }

    #[must_use]
    pub fn offset(&self) -> i64 {
        i64::from(self.page - 1) * i64::from(self.per_page)
    }
}

impl Default for PageRequest {
    fn default() -> Self {
        Self::new(1, DEFAULT_PER_PAGE)
    }
}

#[derive(Deserialize)]
struct RawPageRequest {
    #[serde(default = "first_page")]
    page: u32,
    #[serde(default = "default_per_page")]
    per_page: u32,
}

fn first_page() -> u32 {
    1
}

fn default_per_page() -> u32 {
    DEFAULT_PER_PAGE
}

impl From<RawPageRequest> for PageRequest {
    fn from(raw: RawPageRequest) -> Self {
        Self::new(raw.page, raw.per_page)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub page: u32,
    pub per_page: u32,
    pub total: u64,
    pub total_pages: u64,
}

impl<T> Page<T> {
    #[must_use]
    pub fn new(items: Vec<T>, request: PageRequest, total: u64) -> Self {
        let per_page = u64::from(request.per_page);
        Self {
            items,
            page: request.page,
            per_page: request.per_page,
            total,
            total_pages: total.div_ceil(per_page),
        }
    }

    pub fn map<U>(self, f: impl FnMut(T) -> U) -> Page<U> {
        Page {
            items: self.items.into_iter().map(f).collect(),
            page: self.page,
            per_page: self.per_page,
            total: self.total,
            total_pages: self.total_pages,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CursorRequest {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_per_page", deserialize_with = "clamp_limit")]
    pub limit: u32,
}

impl Default for CursorRequest {
    fn default() -> Self {
        Self {
            cursor: None,
            limit: DEFAULT_PER_PAGE,
        }
    }
}

fn clamp_limit<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    u32::deserialize(deserializer).map(|v| v.clamp(1, MAX_PER_PAGE))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursorPage<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

impl<T> CursorPage<T> {
    pub fn map<U>(self, f: impl FnMut(T) -> U) -> CursorPage<U> {
        CursorPage {
            items: self.items.into_iter().map(f).collect(),
            next_cursor: self.next_cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_is_normalized() {
        let req = PageRequest::new(0, 10_000);
        assert_eq!((req.page(), req.per_page()), (1, MAX_PER_PAGE));
    }

    #[test]
    fn offset_and_limit() {
        let req = PageRequest::new(3, 25);
        assert_eq!((req.limit(), req.offset()), (25, 50));
    }

    #[test]
    fn deserialize_applies_defaults_and_clamp() {
        let req: PageRequest = serde_json::from_str(r#"{"per_page": 500}"#).unwrap();
        assert_eq!(req, PageRequest::new(1, MAX_PER_PAGE));
        let cur: CursorRequest = serde_json::from_str(r#"{"limit": 0}"#).unwrap();
        assert_eq!(cur.limit, 1);
    }

    #[test]
    fn total_pages_rounds_up() {
        let page = Page::new(vec![1, 2], PageRequest::new(1, 20), 41);
        assert_eq!(page.total_pages, 3);
        assert_eq!(page.map(|x| x * 2).items, vec![2, 4]);
    }
}
