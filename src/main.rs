use std::{collections::HashMap, fs::read_to_string, process::exit};

use salsa::{Database, DatabaseImpl};

use crate::{
    ast::{Ident, TypeExprId},
    bytecode::{Func, Prog},
    diagnostic::Diagnostic,
    input::Source,
    lowerer::Builder,
    tp::{Env, FnSig, Type},
    vm::VM,
};

mod ast;
mod bytecode;
mod diagnostic;
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

    let db = &DatabaseImpl::new();

    let text = read_to_string(&filename).expect("couldnt open file");

    let sf = Source::new(db, text.clone());

    let functions = input::parse_file(db, sf).unwrap();

    let diags = input::parse_file::accumulated::<Diagnostic>(db, sf);

    for diag in &diags {
        diag.as_ariadne_report(&filename)
            .eprint((&filename, ariadne::Source::from(&text)))
            .unwrap();
    }

    if !diags.is_empty() {
        eprintln!("errors occured, compilation aborted");
        exit(-1);
    }

    let fn_defs: ModuleDefs<'_> = get_defs(db, sf);

    println!("{:#?}", fn_defs);

    for func in functions.defs(db) {
        let mut env: Env = Env::new(db, &fn_defs.defs);
        for (arg, tp) in func.args(db) {
            env.add_var(arg, parse_type_expr(db, tp))
        }
        let ret_tp = parse_type_expr(db, func.ret(db));
        env.check_expr(func.body(db), &ret_tp)
            .expect("function body doesnt match its declared type");
    }

    let mut compiled_functions: HashMap<String, Func> = HashMap::new();

    for func in functions.defs(db) {
        let mut builder = Builder::new(db);
        for (arg, _) in func.args(db) {
            builder.new_var(arg);
        }
        builder.lower(func.body(db));

        let compiled_func = builder.finish();
        compiled_functions.insert(func.name(db).text(db).clone(), compiled_func);
    }

    let prog = Prog {
        funcs: compiled_functions,
    };

    let mut vm = VM::new(&prog.funcs);

    let res = vm.eval_func("main", 0).unwrap();

    println!("Result: {}", res);
}

#[salsa::tracked]
fn parse_type_expr<'db>(db: &'db dyn Database, tp: TypeExprId<'db>) -> Type {
    match tp.data(db) {
        ast::TypeExprData::Int => Type::Int,
        ast::TypeExprData::Fn(type_expr_ids, type_expr_id) => todo!(),
    }
}

#[derive(Debug, PartialEq, Eq, Clone, salsa::Update)]
pub struct ModuleDefs<'db> {
    defs: HashMap<Ident<'db>, FnSig>,
}

#[salsa::tracked]
fn get_defs<'db>(db: &'db dyn Database, sf: Source) -> ModuleDefs<'db> {
    let mut defs = HashMap::new();

    let file = input::parse_file(db, sf).unwrap();

    for func in file.defs(db) {
        let args = func
            .args(db)
            .into_iter()
            .map(|(_, tp)| parse_type_expr(db, tp))
            .collect();
        let ret = Box::new(parse_type_expr(db, func.ret(db)));
        let sig = FnSig { args, ret };
        defs.insert(func.name(db), sig);
    }

    ModuleDefs { defs }
}
