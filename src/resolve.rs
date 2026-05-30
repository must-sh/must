use crate::{
    ast::{self, FnDef, Ident, TypeExprId},
    input::{self, Source},
    tp::{FnSig, Type, TypeInfo},
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
        ast::TypeExprData::Var(id) => Type::Var(id.get_id()),
        ast::TypeExprData::Array(n, tp) => Type::Array(n, Box::new(parse_type_expr(db, tp))),
        ast::TypeExprData::Slice(tp, is_mut) => {
            let tp = Box::new(parse_type_expr(db, tp));
            Type::Slice(tp, is_mut)
        }
    }
}

#[derive(Debug, PartialEq, Clone, salsa::Update)]
pub struct ModuleDefs<'db> {
    pub function_map: HashMap<Ident<'db>, FnSig>,
    pub type_map: HashMap<usize, TypeInfo<'db>>,
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

pub fn get_defs<'db>(db: &'db dyn Database, sf: Source) -> ModuleDefs<'db> {
    let mut function_map = HashMap::new();
    let mut type_map = HashMap::new();

    let file = input::parse_file(db, sf);

    for def in file.defs(db) {
        match def {
            ast::Def::Fn(func) => {
                let sig = parse_fn_signature(db, func);
                function_map.insert(func.name(db), sig);
            }
            ast::Def::Struct(tp_def) => {
                let name = tp_def.name(db);
                let info = TypeInfo {
                    name,
                    fields: tp_def
                        .fields(db)
                        .into_iter()
                        .enumerate()
                        .map(|(id, (name, tp))| (name, (id, parse_type_expr(db, tp))))
                        .collect(),
                };
                type_map.insert(name.get_id(), info);
            }
        }
    }

    ModuleDefs {
        function_map,
        type_map,
    }
}
