use std::collections::HashMap;

use salsa::Database;

use crate::{
    ast,
    bytecode::{self, FuncSig},
    input::{self, get_source},
    lowerer,
    resolve::{self, parse_fn_signature},
    tp,
};

#[salsa::tracked]
pub fn type_check(db: &dyn Database, c: input::Crate) {
    let root = get_source(db, c, vec![]).unwrap();
    let mut sources = vec![root];
    while let Some(sf) = sources.pop() {
        let ast = input::parse_file(db, sf);

        for def in ast.defs(db) {
            match def {
                ast::Def::Fn(func) => {
                    type_check_func(db, func);
                }
                ast::Def::Struct(_) => (),
                ast::Def::ModuleDecl(ident) => {
                    let sf = get_child_sf(db, sf, ident);
                    sources.push(sf)
                }
            }
        }
    }
    for (_, c) in c.dependencies(db) {
        type_check(db, *c);
    }
}

#[salsa::tracked]
pub fn type_check_func<'db>(
    db: &'db dyn Database,
    func: ast::FnDef<'db>,
) -> tp::InferenceResult<'db> {
    let mut env: tp::Env = tp::Env::new(db, func.sf(db));
    for (arg, tp) in func.args(db) {
        let tp = resolve::parse_type_expr(db, tp, func.sf(db));
        let bindings = env.check_pat(arg, tp);
        env.extend(bindings);
    }
    let ret_tp = resolve::parse_fn_signature(db, func).ret;
    match func.body(db) {
        Some(body) => env.check_expr(body, ret_tp, false),
        None => assert!(func.is_ext(db)),
    }
    env.finish()
}

pub fn get_crate_fns(db: &dyn Database, c: input::Crate) -> HashMap<String, bytecode::FuncSig> {
    let mut fns: HashMap<String, bytecode::FuncSig> = HashMap::new();
    let root = get_source(db, c, vec![]).unwrap();
    let mut sources = vec![root];

    while let Some(sf) = sources.pop() {
        let ast = input::parse_file(db, sf);
        for def in ast.defs(db) {
            match def {
                ast::Def::Fn(func) => {
                    let sig = parse_fn_signature(db, func);
                    let name = if func.is_ext(db) {
                        func.name(db).text(db).clone()
                    } else {
                        resolve::get_fn_full_name(db, sf, func)
                    };

                    fns.insert(name, FuncSig::from_ast_sig(db, sig));
                }
                ast::Def::Struct(_) => (),
                ast::Def::ModuleDecl(ident) => {
                    let sf = get_child_sf(db, sf, ident);
                    sources.push(sf);
                }
            }
        }
    }
    fns
}

pub fn compile(db: &dyn Database, c: input::Crate) -> bytecode::Prog {
    let mut funcs: HashMap<String, bytecode::Func> = HashMap::new();
    let mut externs: HashMap<String, bytecode::FuncSig> = HashMap::new();

    let root = get_source(db, c, vec![]).unwrap();
    let mut sources = vec![root];

    while let Some(sf) = sources.pop() {
        let ast = input::parse_file(db, sf);
        for def in ast.defs(db) {
            match def {
                ast::Def::Fn(func) => match lowerer::Builder::new(db, func).compile() {
                    lowerer::LoweringResult::Function(compiled_func) => {
                        let name = resolve::get_fn_full_name(db, sf, func);
                        funcs.insert(name, compiled_func);
                    }
                    lowerer::LoweringResult::Extern(sig) => {
                        let name = func.name(db).text(db).clone();
                        externs.insert(name, sig);
                    }
                },
                ast::Def::Struct(_) => (),
                ast::Def::ModuleDecl(ident) => {
                    let sf = get_child_sf(db, sf, ident);
                    sources.push(sf);
                }
            }
        }
    }

    for (_, c) in c.dependencies(db) {
        let fns = get_crate_fns(db, *c);
        externs.extend(fns);
    }

    bytecode::Prog { funcs, externs }
}

pub fn get_child_sf(
    db: &(dyn Database + 'static),
    sf: input::Source,
    ident: ast::Ident<'_>,
) -> input::Source {
    let mut path = sf.module_path(db).clone();
    path.push(ident.text(db).clone());
    let sf = get_source(db, sf.c(db), path).unwrap();
    sf
}
