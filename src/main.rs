use std::{fs::read_to_string, path::PathBuf, process::exit};

use clap::Parser;
use salsa::DatabaseImpl;

use crate::{diagnostic::Diagnostic, input::Source};

mod ast;
mod bytecode;
mod codegen;
mod common;
mod diagnostic;
mod driver;
mod input;
mod lowerer;
mod resolve;
mod tp;

lalrpop_util::lalrpop_mod!(parser);

#[derive(clap::Parser)]
struct Cli {
    /// Source file to compile
    path: String,
    #[arg(long, default_value_t = false)]
    no_color: bool,
}

#[derive(Copy, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
enum Ir {
    Cranelift,
    Bytecode,
}

fn main() {
    let cli = Cli::parse();
    let db = &DatabaseImpl::new();
    let root_dir = cli.path;
    let prog = check_and_compile(db, &root_dir, !cli.no_color);

    let obj = codegen::compile(prog, false);
    let bytes = obj.emit().unwrap();
    let mut p = PathBuf::from(root_dir);
    p.set_extension("o");
    std::fs::write(&p, bytes).unwrap();
}

fn check_and_compile(db: &DatabaseImpl, filename: &String, color: bool) -> bytecode::Prog {
    let text = read_to_string(&filename).expect("couldnt open file");
    let sf = Source::new(db, text.clone(), filename.into());
    driver::type_check(db, sf);
    let diags = driver::type_check::accumulated::<Diagnostic>(db, sf);
    for diag in &diags {
        let sf = diag.source;
        let file_name = &sf.file_name(db).to_str().unwrap().to_string();
        diag.as_ariadne_report(file_name, color)
            .eprint((file_name, ariadne::Source::from(&sf.text(db))))
            .unwrap();
    }
    if !diags.is_empty() {
        eprintln!("errors occured, compilation aborted");
        exit(-1);
    }
    driver::compile(db, sf)
}
