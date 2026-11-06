use crate::{
    common::{Binop, Unop},
    input::Source,
    tp::TypeId,
};

#[salsa::tracked(debug)]
pub struct Span<'db> {
    #[returns(clone)]
    pub start_byte: usize,
    #[returns(clone)]
    pub end_byte: usize,
}

#[salsa::tracked(debug)]
pub struct File<'db> {
    #[returns(clone)]
    pub defs: Vec<Def<'db>>,
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, salsa::SalsaValue)]
pub enum Def<'db> {
    Fn(FnDef<'db>),
    Struct(StructDef<'db>),
    Import(StrLit<'db>),
    Static {
        name: Ident<'db>,
        is_mut: bool,
        tp: TypeId<'db>,
        value: ExprId<'db>,
    },
}

#[salsa::tracked(debug)]
pub struct StructDef<'db> {
    #[returns(clone)]
    pub name: Ident<'db>,
    #[returns(clone)]
    pub span: Span<'db>,
    #[returns(clone)]
    pub fields: Vec<(Ident<'db>, TypeId<'db>)>,
    #[returns(clone)]
    pub sf: Source,
}

#[salsa::tracked(debug)]
pub struct FnDef<'db> {
    #[returns(clone)]
    pub is_ext: bool,
    pub name: Ident<'db>,
    #[returns(clone)]
    pub span: Span<'db>,
    #[returns(clone)]
    pub args: Vec<(PatternId<'db>, TypeId<'db>)>,
    #[returns(clone)]
    pub ret: Option<TypeId<'db>>,
    #[returns(clone)]
    pub body: Option<ExprId<'db>>,
    #[returns(clone)]
    pub sf: Source,
}

#[salsa::interned(debug)]
pub struct ExprId {
    #[returns(clone)]
    pub data: ExprData<'db>,
    #[returns(clone)]
    pub span: Span<'db>,
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, salsa::SalsaValue)]
pub enum ExprData<'db> {
    Int(u64),
    Float(StrLit<'db>),
    Bool(bool),
    Char(u8),
    Str(StrLit<'db>),
    Binop(Binop, ExprId<'db>, ExprId<'db>),
    Unop(Unop, ExprId<'db>),
    Let(PatternId<'db>, ExprId<'db>, ExprId<'db>),
    Var(Ident<'db>),
    FnCall(ExprId<'db>, Vec<ExprId<'db>>),
    If(ExprId<'db>, ExprId<'db>, Option<ExprId<'db>>),
    While(ExprId<'db>, ExprId<'db>),
    Assign(ExprId<'db>, ExprId<'db>),

    Deref(ExprId<'db>),
    AddressOf(ExprId<'db>),

    Tuple(Vec<ExprId<'db>>),
    Seq(ExprId<'db>, ExprId<'db>),

    Struct(Ident<'db>, Vec<(Ident<'db>, ExprId<'db>)>),
    Field(ExprId<'db>, Ident<'db>),

    Array(Vec<ExprId<'db>>),
    Index(ExprId<'db>, ExprId<'db>),

    Range(ExprId<'db>, ExprId<'db>),

    Cast(ExprId<'db>, TypeId<'db>),

    Error,
}

#[salsa::interned(debug)]
pub struct PatternId {
    #[returns(clone)]
    pub data: PatternData<'db>,
    #[returns(clone)]
    pub span: Span<'db>,
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, salsa::SalsaValue)]
pub enum PatternData<'db> {
    Wildcard,
    Var(Ident<'db>, bool),
    Tuple(Vec<PatternId<'db>>),
}

#[salsa::interned(debug)]
pub struct Ident {
    #[returns(ref)]
    pub text: String,
}

#[salsa::interned(debug)]
pub struct StrLit {
    #[returns(ref)]
    pub text: String,
}
