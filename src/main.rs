use std::{collections::HashMap, fs::read_to_string, process::exit};

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

#[derive(Debug, serde::Deserialize)]
struct CrateConfig {
    package: PackageInfo,
    dependencies: HashMap<String, Dependency>,
}

#[derive(Debug, serde::Deserialize)]
struct PackageInfo {
    name: String,
    owner: String,
    kind: PackageKind,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum PackageKind {
    Exe,
    Lib,
}

#[derive(Debug, serde::Deserialize)]
struct Dependency {
    path: String,
}

fn main() {
    let cli = Cli::parse();
    let db = &DatabaseImpl::new();
    let root_dir = cli.path.unwrap_or("".into());
    let cfg_file = format!("{}/must.toml", root_dir);
    let cfg: CrateConfig = toml::from_str(&read_to_string(cfg_file).unwrap()).unwrap();
    let prog = check_and_compile(db, &root_dir);
    match cli.cmd {
        Command::Run => {
            let mut vm = VM::new(&prog.funcs);
            match vm.eval_func("main") {
                Some(_) => println!("Result: {:?}", vm.finish()),
                None => println!("runtime error occured!"),
            }
        }
        Command::Compile => {
            let obj = codegen::compile(prog, false);
            let bytes = obj.emit().unwrap();
            let target = root_dir + "build/";
            if !std::fs::exists(&target).unwrap() {
                std::fs::create_dir(&target).unwrap();
            }
            std::fs::write(target + "a.out", bytes).unwrap()
        }
        Command::Print { ir } => {
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
    let c = get_crate(db, root_dir);
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

fn get_crate(db: &DatabaseImpl, root_dir: &String) -> input::Crate {
    let cfg_file = format!("{}/must.toml", root_dir);
    let p = std::path::Path::new(&cfg_file).canonicalize().unwrap();
    println!("{}: {:?}", cfg_file, p);
    let cfg: CrateConfig = toml::from_str(&read_to_string(cfg_file).unwrap()).unwrap();
    let deps = get_deps(db, root_dir, cfg);
    input::Crate::new(db, root_dir.clone().into(), deps)
}

fn get_deps(
    db: &DatabaseImpl,
    root_dir: &String,
    cfg: CrateConfig,
) -> HashMap<String, input::Crate> {
    let mut map = HashMap::new();
    for (name, dep) in cfg.dependencies {
        let mut path = root_dir.clone();
        path += &dep.path;
        let c = get_crate(db, &path);
        map.insert(name, c);
    }
    map
}
