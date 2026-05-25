use crate::{
    ast::{self, FnDef, Ident, TypeExprId},
    input::{self, Source},
    tp::{FnSig, Type},
};
use salsa::Database;
use std::collections::HashMap;

#[salsa::tracked]
pub(crate) fn parse_type_expr<'db>(db: &'db dyn Database, tp: TypeExprId<'db>) -> Type {
    match tp.data(db) {
        ast::TypeExprData::Int => Type::Int,
        ast::TypeExprData::Bool => Type::Bool,
        ast::TypeExprData::Fn(args, ret) => {
            let args = args
                .into_iter()
                .map(|arg| parse_type_expr(db, arg))
                .collect();
            let ret = Box::new(parse_type_expr(db, ret));
            let fn_sig = FnSig { args, ret };
            Type::Fn(fn_sig)
        }
        ast::TypeExprData::Ptr(tp, is_mut) => {
            let tp = Box::new(parse_type_expr(db, tp));
            Type::Ptr(tp, is_mut)
        }
        ast::TypeExprData::Tuple(tps) => {
            let tps = tps.into_iter().map(|tp| parse_type_expr(db, tp)).collect();
            Type::Tuple(tps)
        }
    }
}

#[derive(Debug, PartialEq, Clone, salsa::Update)]
pub struct ModuleDefs<'db> {
    pub(crate) defs: HashMap<Ident<'db>, FnSig>,
}

#[salsa::tracked]
pub fn parse_fn_signature<'db>(db: &'db dyn Database, func: FnDef<'db>) -> FnSig {
    let args = func
        .args(db)
        .into_iter()
        .map(|(_, tp)| parse_type_expr(db, tp))
        .collect();
    let ret = if let Some(tp) = func.ret(db) {
        parse_type_expr(db, tp)
    } else {
        Type::Tuple(vec![])
    };
    let ret = Box::new(ret);
    FnSig { args, ret }
}

#[salsa::tracked]
pub(crate) fn get_defs<'db>(db: &'db dyn Database, sf: Source) -> ModuleDefs<'db> {
    let mut defs = HashMap::new();

    let file = input::parse_file(db, sf);

    for func in file.defs(db) {
        let sig = parse_fn_signature(db, func);
        defs.insert(func.name(db), sig);
    }

    ModuleDefs { defs }
}
