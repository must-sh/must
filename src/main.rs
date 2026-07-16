use std::process::exit;

use clap::Parser;
use salsa::DatabaseImpl;

use crate::{diagnostic::Diagnostic, vm::VM};

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
struct Cli {
    #[command(subcommand)]
    cmd: Command,
    #[arg(long)]
    path: Option<String>,
}

#[derive(Debug, Clone, clap::Subcommand)]
enum Command {
    /// Run a file.
    Run,
    /// Compile a file.
    Compile,
    /// Print intermediate representation.
    Print { ir: Ir },
}

#[derive(Copy, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, clap::ValueEnum)]
enum Ir {
    Cranelift,
    Bytecode,
}

fn main() {
    let cli = Cli::parse();
    let db = &DatabaseImpl::new();
    let root_dir = cli.path.unwrap_or("".into());
    match cli.cmd {
        Command::Run => {
            let prog = check_and_compile(db, &root_dir);
            let mut vm = VM::new(&prog.funcs);
            match vm.eval_func("main") {
                Some(_) => println!("Result: {:?}", vm.finish()),
                None => println!("runtime error occured!"),
            }
        }
        Command::Compile => {
            let prog = check_and_compile(db, &root_dir);
            let obj = codegen::compile(prog, false);
            let bytes = obj.emit().unwrap();
            let target = root_dir + "build/";
            if !std::fs::exists(&target).unwrap() {
                std::fs::create_dir(&target).unwrap();
            }
            std::fs::write(target + "a.out", bytes).unwrap()
        }
        Command::Print { ir } => {
            let prog = check_and_compile(db, &root_dir);
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

fn check_and_compile(db: &DatabaseImpl, root_dir: &String) -> bytecode::Prog {
    let c = input::Crate::new(db, root_dir.clone().into());
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
