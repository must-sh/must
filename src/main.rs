use std::{fs::read_to_string, process::exit};

use salsa::DatabaseImpl;

use crate::{diagnostic::Diagnostic, input::Source, vm::VM};

mod ast;
mod bytecode;
mod common;
mod diagnostic;
mod driver;
mod input;
mod lowerer;
mod resolve;
mod tp;
mod vm;

lalrpop_util::lalrpop_mod!(parser);

fn main() {
    let filename = match &std::env::args().collect::<Vec<String>>()[..] {
        [_prog_name, filename] => filename.clone(),
        _ => panic!("i take a filename as argument"),
    };

    let db = &DatabaseImpl::new();

    let text = read_to_string(&filename).expect("couldnt open file");

    let sf = Source::new(db, text.clone());

    let functions = input::parse_file(db, sf);

    driver::type_check_file(db, sf);

    let diags = driver::type_check_file::accumulated::<Diagnostic>(db, sf);

    for diag in &diags {
        diag.as_ariadne_report(&filename)
            .eprint((&filename, ariadne::Source::from(&text)))
            .unwrap();
    }

    if !diags.is_empty() {
        eprintln!("errors occured, compilation aborted");
        exit(-1);
    }

    let prog = driver::compile(db, functions);

    println!("{prog:#?}");

    let mut vm = VM::new(&prog.funcs);

    match vm.eval_func("main") {
        Some(_) => println!("Result: {:?}", vm.finish()),
        None => println!("runtime error occured!"),
    }
}
