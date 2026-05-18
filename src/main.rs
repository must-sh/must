use std::collections::HashMap;

use crate::{
    bytecode::{Func, Prog},
    lowerer::Builder,
    vm::VM,
};

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

    let functions = input::parse_file(filename).unwrap().defs;

    let mut compiled_functions: HashMap<String, Func> = HashMap::new();

    for func in functions {
        let mut builder = Builder::new();
        for arg in func.args {
            builder.new_var(arg);
        }
        builder.lower(func.body);

        let compiled_func = builder.finish();
        compiled_functions.insert(func.name, compiled_func);
    }

    let prog = Prog {
        funcs: compiled_functions,
    };

    let mut vm = VM::new(&prog.funcs);

    let res = vm.eval_func("main", 0).unwrap();

    println!("Result: {}", res);
}
