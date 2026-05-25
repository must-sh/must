use salsa::{Accumulator, Database};

use crate::{ast::File, diagnostic::Diagnostic, parser};

#[salsa::input(debug)]
pub struct Source {
    #[returns(ref)]
    text: String,
}

#[salsa::tracked]
pub fn parse_file<'db>(db: &'db dyn Database, source: Source) -> File<'db> {
    let parser = parser::FileParser::new();
    match parser.parse(db, source, source.text(db)) {
        Ok(file) => file,
        Err(e) => {
            Diagnostic::parser_error(e).accumulate(db);
            File::new(db, vec![])
        }
    }
}
