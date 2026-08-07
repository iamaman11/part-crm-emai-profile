use contracts::ProblemCode;
use core::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApplicationError {
    code: ProblemCode,
}

impl ApplicationError {
    #[must_use]
    pub const fn new(code: ProblemCode) -> Self {
        Self { code }
    }

    #[must_use]
    pub const fn code(self) -> ProblemCode {
        self.code
    }
}

impl fmt::Display for ApplicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.stable_code())
    }
}

impl std::error::Error for ApplicationError {}
