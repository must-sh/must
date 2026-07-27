use crate::{
    common::{Binop, Unop},
    input::Source,
    tp::TypeId,
};

#[salsa::tracked(debug)]
pub struct Span<'db> {
    #[tracked(ref)]
    pub start_byte: usize,
    #[tracked(ref)]
    pub end_byte: usize,
}

#[salsa::tracked(debug)]
pub struct File<'db> {
    pub defs: Vec<Def<'db>>,
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, salsa::Update)]
pub enum Def<'db> {
    Fn(FnDef<'db>),
    Struct(StructDef<'db>),
    Import(StrLit<'db>),
}

#[salsa::tracked(debug)]
pub struct StructDef<'db> {
    pub name: Ident<'db>,
    pub span: Span<'db>,
    pub fields: Vec<(Ident<'db>, TypeId<'db>)>,
    pub sf: Source,
}

#[salsa::tracked(debug)]
pub struct FnDef<'db> {
    pub is_ext: bool,
    pub name: Ident<'db>,
    pub span: Span<'db>,
    pub args: Vec<(PatternId<'db>, TypeId<'db>)>,
    pub ret: Option<TypeId<'db>>,
    pub body: Option<ExprId<'db>>,
    pub sf: Source,
}

#[salsa::interned(debug)]
pub struct ExprId {
    pub data: ExprData<'db>,
    pub span: Span<'db>,
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, salsa::Update)]
pub enum ExprData<'db> {
    Number(i64),
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
    pub data: PatternData<'db>,
    pub span: Span<'db>,
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, salsa::Update)]
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
