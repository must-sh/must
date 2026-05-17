use crate::{lowerer::Builder, vm::eval};

mod ast;
mod bytecode;
mod input;
mod lowerer;
mod vm;

lalrpop_util::lalrpop_mod!(parser);

fn main() {
    let filename = match &std::env::args().collect::<Vec<String>>()[..] {
        [_prog_name, filename] => filename.clone(),
        _ => panic!("i take a filename as argument"),
    };

    let ast = input::parse_file(filename).unwrap().expr;

    let mut builder = Builder::new();

    builder.lower(ast);

    let prog = builder.finish();

    let res = eval(prog).unwrap();

    println!("Result: {}", res);
}
