use std::collections::HashMap;

use salsa::Database;

use crate::{ast, bytecode, input, lowerer, resolve, tp};

#[salsa::tracked]
pub fn type_check_file<'db>(db: &'db dyn Database, sf: input::Source) {
    let functions = input::parse_file(db, sf);
    let fn_defs: resolve::ModuleDefs<'_> = resolve::get_defs(db, sf);

    for func in functions.defs(db) {
        let mut env: tp::Env = tp::Env::new(db, &fn_defs.defs);
        for (arg, tp) in func.args(db) {
            let tp = resolve::parse_type_expr(db, tp);
            let bindings = env.check_pat(arg, tp);
            env.extend(bindings);
        }
        let ret_tp = resolve::parse_type_expr(db, func.ret(db));
        match func.body(db) {
            Some(body) => env.check_expr(body, &ret_tp, false),
            None => assert!(func.is_ext(db)),
        }
    }
}

#[salsa::tracked]
pub fn compile<'db>(db: &'db dyn Database, functions: ast::File<'db>) -> bytecode::Prog {
    let mut compiled_functions: HashMap<String, bytecode::Func> = HashMap::new();

    for func in functions.defs(db) {
        if let Some(body) = func.body(db) {
            let mut builder = lowerer::Builder::new(db);
            for (arg, _) in func.args(db) {
                builder.lower_pat(arg);
            }
            builder.lower(body);

            let compiled_func = builder.finish();
            compiled_functions.insert(func.name(db).text(db).clone(), compiled_func);
        }
    }

    let prog = bytecode::Prog {
        funcs: compiled_functions,
    };
    prog
}
