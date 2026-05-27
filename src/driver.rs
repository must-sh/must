use std::collections::HashMap;

use salsa::Database;

use crate::{
    ast::{self, FnDef},
    bytecode, input, lowerer,
    resolve::{self, parse_type_expr},
    tp::{self, InferenceResult},
};

#[salsa::tracked]
pub fn type_check_file(db: &dyn Database, sf: input::Source) {
    let functions = input::parse_file(db, sf);

    for func in functions.defs(db) {
        type_check_func(db, func);
    }
}

#[salsa::tracked]
pub fn type_check_func<'db>(db: &'db dyn Database, func: FnDef<'db>) -> InferenceResult<'db> {
    let defs = resolve::get_defs(db, func.sf(db)).defs;
    let mut env: tp::Env = tp::Env::new(db, defs);
    for (arg, tp) in func.args(db) {
        let tp = resolve::parse_type_expr(db, tp);
        let bindings = env.check_pat(arg, &tp);
        env.extend(bindings);
    }
    let ret_tp = resolve::parse_fn_signature(db, func).ret;
    match func.body(db) {
        Some(body) => env.check_expr(body, &ret_tp, false),
        None => assert!(func.is_ext(db)),
    }
    env.finish()
}

#[salsa::tracked]
pub fn compile<'db>(db: &'db dyn Database, functions: ast::File<'db>) -> bytecode::Prog {
    let mut compiled_functions: HashMap<String, bytecode::Func> = HashMap::new();

    for func in functions.defs(db) {
        if let Some(body) = func.body(db) {
            let type_map = type_check_func(db, func).type_map;
            let mut builder = lowerer::Builder::new(db, &type_map);
            for (arg, tp) in func.args(db) {
                let tp = parse_type_expr(db, tp);
                builder.lower_pat(arg, &tp);
            }
            builder.lower(body);

            let compiled_func = builder.finish();
            compiled_functions.insert(func.name(db).text(db).clone(), compiled_func);
        }
    }

    
    bytecode::Prog {
        funcs: compiled_functions,
    }
}
