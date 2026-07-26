use std::{collections::HashMap, fs::read_to_string};

use salsa::Database;

use crate::{
    ast,
    bytecode::{self, FuncSig},
    input::{self, resolve_import},
    lowerer,
    resolve::{self, parse_fn_signature},
    tp,
};

#[salsa::tracked]
pub fn type_check(db: &dyn Database, sf: input::Source) {
    let mut sources = vec![sf];
    while let Some(sf) = sources.pop() {
        let ast = input::parse_file(db, sf);

        for def in ast.defs(db) {
            match def {
                ast::Def::Fn(func) => {
                    type_check_func(db, func);
                }
                ast::Def::Struct(_) => (),
                ast::Def::Import(name) => {
                    let sf = get_child_sf(db, sf, name);
                    type_check(db, sf);
                }
            }
        }
    }
}

#[salsa::tracked]
pub fn type_check_func<'db>(
    db: &'db dyn Database,
    func: ast::FnDef<'db>,
) -> tp::InferenceResult<'db> {
    let mut env: tp::Env = tp::Env::new(db, func.sf(db));
    for (arg, tp) in func.args(db) {
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

pub fn compile(db: &dyn Database, sf: input::Source) -> bytecode::Prog {
    let mut funcs: HashMap<String, bytecode::Func> = HashMap::new();
    let mut externs: HashMap<String, bytecode::FuncSig> = HashMap::new();

    let mut sources = vec![sf];

    while let Some(sf) = sources.pop() {
        let ast = input::parse_file(db, sf);
        for def in ast.defs(db) {
            match def {
                ast::Def::Fn(func) => match lowerer::Builder::new(db, func).compile() {
                    lowerer::LoweringResult::Function(compiled_func) => {
                        let name = func.name(db).text(db).clone();
                        funcs.insert(name, compiled_func);
                    }
                    lowerer::LoweringResult::Extern(sig) => {
                        let name = func.name(db).text(db).clone();
                        externs.insert(name, sig);
                    }
                },
                ast::Def::Struct(_) => (),
                ast::Def::Import(ident) => {
                    let sf = get_child_sf(db, sf, ident);
                    sources.push(sf);
                }
            }
        }
    }

    bytecode::Prog { funcs, externs }
}

pub fn get_child_sf(
    db: &(dyn Database + 'static),
    sf: input::Source,
    name: ast::StrLit<'_>,
) -> input::Source {
    let mut path = sf.file_name(db).clone();
    path.pop();
    path.push(name.text(db).clone());
    let text = read_to_string(&path).expect("couldnt open file");
    let sf = input::Source::new(db, text.clone(), path.into());
    sf
}
