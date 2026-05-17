use std::{fs::read_to_string, path::Path};

use crate::{ast::File, parser};

pub fn parse_file(path: impl AsRef<Path>) -> Option<File> {
    let text = read_to_string(path).ok()?;
    let parser = parser::FileParser::new();
    parser.parse(&text).ok()
}
