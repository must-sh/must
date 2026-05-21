use std::{fs::read_to_string, path::Path};

use salsa::Database;

use crate::{ast::File, parser};

#[salsa::input]
pub struct Source {
    #[returns(ref)]
    text: String,
}

#[salsa::tracked]
pub fn parse_file<'db>(db: &'db dyn Database, source: Source) -> Option<File<'db>> {
    let parser = parser::FileParser::new();
    parser.parse(db, source.text(db)).ok()
}
