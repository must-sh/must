use std::{fs::read_to_string, process::exit};

use clap::Parser;
use salsa::DatabaseImpl;

use crate::{
    diagnostic::Diagnostic,
    input::{Crate, Source, get_source},
    vm::VM,
};

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
mod vm;

lalrpop_util::lalrpop_mod!(parser);

#[derive(clap::Parser)]
enum Cli {
    /// Run a file.
    Run { file_name: String },
    /// Compile a file.
    Compile { file_name: String },
    /// Print intermediate representation.
    Print { ir: Ir, file_name: String },
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
enum Ir {
    Cranelift,
    Bytecode,
}

fn main() {
    let cli = Cli::parse();
    let db = &DatabaseImpl::new();
    match cli {
        Cli::Run { file_name } => {
            let prog = check_and_compile(db, file_name);
            let mut vm = VM::new(&prog.funcs);
            match vm.eval_func("main") {
                Some(_) => println!("Result: {:?}", vm.finish()),
                None => println!("runtime error occured!"),
            }
        }
        Cli::Compile { file_name } => {
            let prog = check_and_compile(db, file_name);
            let obj = codegen::compile(prog, false);
            let bytes = obj.emit().unwrap();
            std::fs::write("a.out", bytes).unwrap()
        }
        Cli::Print { ir, file_name } => {
            let prog = check_and_compile(db, file_name);
            match ir {
                Ir::Cranelift => {
                    codegen::compile(prog, true);
                }
                Ir::Bytecode => {
                    println!("{}", prog);
                }
            };
        }
    }
}

fn check_and_compile(db: &DatabaseImpl, root_dir: String) -> bytecode::Prog {
    let c = Crate::new(db, root_dir.clone().into());
    driver::type_check(db, c);
    let diags = driver::type_check::accumulated::<Diagnostic>(db, c);
    for diag in &diags {
        let sf = diag.source;
        let file_name = &sf.file_name(db).to_str().unwrap().to_string();
        diag.as_ariadne_report(file_name)
            .eprint((file_name, ariadne::Source::from(&sf.text(db))))
            .unwrap();
    }
    if !diags.is_empty() {
        eprintln!("errors occured, compilation aborted");
        exit(-1);
    }
    driver::compile(db, c)
}
