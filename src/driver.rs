use std::collections::HashMap;

use salsa::Database;

use crate::{ast, bytecode, input, lowerer, resolve, tp};

#[salsa::tracked]
pub fn type_check_file(db: &dyn Database, sf: input::Source) {
    let ast = input::parse_file(db, sf);

    for def in ast.defs(db) {
        match def {
            ast::Def::Fn(func) => {
                type_check_func(db, func);
            }
            ast::Def::Struct(_) => (),
        }
    }
}

#[salsa::tracked]
pub fn type_check_func<'db>(
    db: &'db dyn Database,
    func: ast::FnDef<'db>,
) -> tp::InferenceResult<'db> {
    let defs = resolve::get_defs(db, func.sf(db));
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

pub fn compile<'db>(db: &'db dyn Database, sf: input::Source) -> bytecode::Prog {
    let ast = input::parse_file(db, sf);
    let mut funcs: HashMap<String, bytecode::Func> = HashMap::new();
    let mut externs: HashMap<String, bytecode::FuncSig> = HashMap::new();

    for def in ast.defs(db) {
        match def {
            ast::Def::Fn(func) => {
                let name = func.name(db).text(db).clone();
                match lowerer::Builder::new(db, func).compile() {
                    lowerer::LoweringResult::Function(compiled_func) => {
                        funcs.insert(name, compiled_func);
                    }
                    lowerer::LoweringResult::Extern(sig) => {
                        externs.insert(name, sig);
                    }
                }
            }
            ast::Def::Struct(_) => (),
        }
    }

    bytecode::Prog { funcs, externs }
}
