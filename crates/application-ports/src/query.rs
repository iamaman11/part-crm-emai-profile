use core::{fmt, future::Future};
use profile_platform_primitives::ActorContext;

pub const MAX_QUERY_PAGE_SIZE: u16 = 100;
const MAX_QUERY_CURSOR_LENGTH: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryCapability {
    Clients,
    Profiles,
    Members,
    Mailboxes,
    Mail,
    GlobalSearch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueryPageSize(u16);

impl QueryPageSize {
    pub fn new(value: u16) -> Result<Self, QueryInputError> {
        if value == 0 || value > MAX_QUERY_PAGE_SIZE {
            return Err(QueryInputError::InvalidPageSize);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryCursor(String);

impl QueryCursor {
    pub fn parse(value: impl Into<String>) -> Result<Self, QueryInputError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_QUERY_CURSOR_LENGTH
            || value.chars().any(char::is_control)
        {
            return Err(QueryInputError::InvalidCursor);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryPageRequest {
    limit: QueryPageSize,
    cursor: Option<QueryCursor>,
}

impl QueryPageRequest {
    #[must_use]
    pub const fn new(limit: QueryPageSize, cursor: Option<QueryCursor>) -> Self {
        Self { limit, cursor }
    }

    #[must_use]
    pub const fn limit(&self) -> QueryPageSize {
        self.limit
    }

    #[must_use]
    pub const fn cursor(&self) -> Option<&QueryCursor> {
        self.cursor.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryPage<T> {
    items: Vec<T>,
    next_cursor: Option<QueryCursor>,
}

impl<T> QueryPage<T> {
    #[must_use]
    pub const fn new(items: Vec<T>, next_cursor: Option<QueryCursor>) -> Self {
        Self { items, next_cursor }
    }

    #[must_use]
    pub const fn empty() -> Self {
        Self {
            items: Vec::new(),
            next_cursor: None,
        }
    }

    #[must_use]
    pub fn items(&self) -> &[T] {
        &self.items
    }

    #[must_use]
    pub const fn next_cursor(&self) -> Option<&QueryCursor> {
        self.next_cursor.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryInputError {
    InvalidPageSize,
    InvalidCursor,
}

impl fmt::Display for QueryInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidPageSize => "query page size is outside the accepted bound",
            Self::InvalidCursor => "query cursor is invalid",
        })
    }
}

impl std::error::Error for QueryInputError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryPortErrorClass {
    IntegrityFailure,
    DependencyUnavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueryPortError {
    class: QueryPortErrorClass,
}

impl QueryPortError {
    #[must_use]
    pub const fn new(class: QueryPortErrorClass) -> Self {
        Self { class }
    }

    #[must_use]
    pub const fn class(self) -> QueryPortErrorClass {
        self.class
    }
}

impl fmt::Display for QueryPortError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.class {
            QueryPortErrorClass::IntegrityFailure => "query projection integrity failure",
            QueryPortErrorClass::DependencyUnavailable => "query projection dependency unavailable",
        })
    }
}

impl std::error::Error for QueryPortError {}

pub trait QueryAuthorizationPort {
    fn is_query_authorized(
        &self,
        actor: &ActorContext,
        capability: QueryCapability,
    ) -> impl Future<Output = Result<bool, QueryPortError>>;
}

#[cfg(test)]
mod tests {
    use super::{MAX_QUERY_PAGE_SIZE, QueryCursor, QueryInputError, QueryPageSize};

    #[test]
    fn query_bounds_fail_closed() {
        assert_eq!(QueryPageSize::new(0), Err(QueryInputError::InvalidPageSize));
        assert_eq!(
            QueryPageSize::new(MAX_QUERY_PAGE_SIZE + 1),
            Err(QueryInputError::InvalidPageSize)
        );
        assert_eq!(
            QueryCursor::parse("bad\ncursor"),
            Err(QueryInputError::InvalidCursor)
        );
    }
}
