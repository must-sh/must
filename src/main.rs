use std::collections::HashMap;

use crate::{
    bytecode::{Func, Prog},
    lowerer::Builder,
    tp::{Env, FnSig},
    vm::VM,
};

mod ast;
mod bytecode;
mod input;
mod lowerer;
mod tp;
mod vm;

lalrpop_util::lalrpop_mod!(parser);

fn main() {
    let filename = match &std::env::args().collect::<Vec<String>>()[..] {
        [_prog_name, filename] => filename.clone(),
        _ => panic!("i take a filename as argument"),
    };

    let functions = input::parse_file(filename).unwrap().defs;

    let fn_defs: HashMap<String, FnSig> = get_defs(&functions);

    println!("{:#?}", fn_defs);

    for func in &functions {
        let mut env: Env = Env::new(&fn_defs);
        for (arg, tp) in &func.args {
            env.add_var(arg.clone(), tp.clone())
        }
        env.check_expr(&func.body, &func.ret)
            .expect("function body doesnt match its declared type");
    }

    let mut compiled_functions: HashMap<String, Func> = HashMap::new();

    for func in functions {
        let mut builder = Builder::new();
        for (arg, _) in func.args {
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

fn get_defs(functions: &[ast::FnDef]) -> HashMap<String, FnSig> {
    let mut def_map = HashMap::new();

    for func in functions {
        let args = func.args.iter().map(|(_, tp)| tp.clone()).collect();
        let ret = Box::new(func.ret.clone());
        let sig = FnSig { args, ret };
        def_map.insert(func.name.clone(), sig);
    }

    def_map
}
