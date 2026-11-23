//! The ActiveModel validators that lower to data.

use crate::record::Record;
use crate::value::Value;

#[derive(Debug, Clone, Copy)]
pub enum Validator {
    /// `validates :col, presence: true`.
    Presence { column: usize, attr: &'static str },
    /// `validates :col, length: { maximum: n }` (and/or `minimum:`). `nil`
    /// passes unless `allow_nil: false` — ActiveModel's length validator skips
    /// nil only when told to, but a nil has no length and fails `minimum`.
    Length {
        column: usize,
        attr: &'static str,
        min: Option<usize>,
        max: Option<usize>,
    },
    /// A validator the slice does not compile (`validates_with SomeClass`);
    /// it occupies its chain position and the host answers for it.
    Host,
    /// R-BELONGS-TO-REQUIRED — loads the association (through the reader's
    /// cache) and requires it to be present. Run by the lifecycle, which has
    /// the context to load records.
    BelongsToRequired {
        assoc: &'static str,
        fk: usize,
        target: &'static crate::lifecycle::ModelSpec,
        only_if_fk_missing_or_changed: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    pub attr: &'static str,
    pub kind: &'static str,
}

impl Validator {
    pub fn run(&self, rec: &Record, errors: &mut Vec<ValidationError>) {
        match *self {
            Self::Presence { column, attr } => {
                if rec.get(column).blank() {
                    errors.push(ValidationError { attr, kind: "blank" });
                }
            }
            Self::Length {
                column,
                attr,
                min,
                max,
            } => {
                let len = match rec.get(column) {
                    Value::Str(s) => Some(s.chars().count()),
                    Value::Nil => None,
                    other => Some(other.to_string().chars().count()),
                };
                match len {
                    None => {}
                    Some(n) => {
                        if max.is_some_and(|m| n > m) {
                            errors.push(ValidationError {
                                attr,
                                kind: "too_long",
                            });
                        }
                        if min.is_some_and(|m| n < m) {
                            errors.push(ValidationError {
                                attr,
                                kind: "too_short",
                            });
                        }
                    }
                }
            }
            Self::Host | Self::BelongsToRequired { .. } => {}
        }
    }
}
