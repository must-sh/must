use std::{fmt::Display, ops::Range};

use lalrpop_util::ParseError;

use crate::ast::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Error,
}

#[salsa::accumulator]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub start_byte: usize,
    pub end_byte: usize,
    pub message: String,
    pub notes: Vec<String>,
}

impl Diagnostic {
    pub fn error(db: &dyn salsa::Database, span: Span, message: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            start_byte: span.start_byte(db),
            end_byte: span.end_byte(db),
            message: message.into(),
            notes: vec![],
        }
    }

    pub fn as_ariadne_report<'a>(
        &self,
        filename: &'a String,
    ) -> ariadne::Report<'a, (&'a String, Range<usize>)> {
        let mut builder = ariadne::Report::build(
            ariadne::ReportKind::Error,
            (filename, self.start_byte..self.end_byte),
        )
        .with_message(&self.message)
        .with_label(
            ariadne::Label::new((filename, self.start_byte..self.end_byte))
                .with_message(&self.message),
        );
        builder.with_notes(&self.notes);
        builder.finish()
    }

    pub fn parser_error<T: Display, E>(err: ParseError<usize, T, E>) -> Self {
        match err {
            lalrpop_util::ParseError::InvalidToken { location } => Self {
                severity: Severity::Error,
                start_byte: location,
                end_byte: location,
                message: "invalid token".into(),
                notes: vec![],
            },
            lalrpop_util::ParseError::UnrecognizedEof { location, expected } => Self {
                severity: Severity::Error,
                start_byte: location,
                end_byte: location,
                message: "unexpected end of file".into(),
                notes: vec![format!("expected one of:\n{}", expected.join("\n"))],
            },
            lalrpop_util::ParseError::UnrecognizedToken { token, expected } => Self {
                severity: Severity::Error,
                start_byte: token.0,
                end_byte: token.2,
                message: format!("unexpected token: {}", token.1),
                notes: vec![format!("expected one of:\n{}", expected.join("\n"))],
            },
            lalrpop_util::ParseError::ExtraToken { token } => Self {
                severity: Severity::Error,
                start_byte: token.0,
                end_byte: token.2,
                message: format!("unexpected token: {}", token.1),
                notes: vec![],
            },
            lalrpop_util::ParseError::User { .. } => {
                todo!("no user-defined error in the parser")
            }
        }
    }
}
