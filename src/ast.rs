#[salsa::tracked(debug)]
pub struct Span<'db> {
    #[tracked]
    pub start_byte: usize,
    #[tracked]
    pub end_byte: usize,
}

#[salsa::tracked(debug)]
pub struct File<'db> {
    pub defs: Vec<FnDef<'db>>,
}

#[salsa::tracked(debug)]
pub struct FnDef<'db> {
    pub is_ext: bool,
    pub name: Ident<'db>,
    pub span: Span<'db>,
    pub args: Vec<(Ident<'db>, TypeExprId<'db>)>,
    pub ret: TypeExprId<'db>,
    pub body: Option<ExprId<'db>>,
}

#[salsa::interned(debug)]
pub struct ExprId {
    pub data: ExprData<'db>,
    pub span: Span<'db>,
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, salsa::Update)]
pub enum ExprData<'db> {
    Number(i32),
    Add(ExprId<'db>, ExprId<'db>),
    Sub(ExprId<'db>, ExprId<'db>),
    Mul(ExprId<'db>, ExprId<'db>),
    Div(ExprId<'db>, ExprId<'db>),
    Let(Ident<'db>, ExprId<'db>, ExprId<'db>),
    Var(Ident<'db>),
    FnCall(Ident<'db>, Vec<ExprId<'db>>),

    Error,
}

#[salsa::interned(debug)]
pub struct TypeExprId {
    pub data: TypeExprData<'db>,
    pub span: Span<'db>,
}

#[derive(Debug, Hash, Eq, PartialEq, Clone, salsa::Update)]
pub enum TypeExprData<'db> {
    Int,
    Fn(Vec<TypeExprId<'db>>, TypeExprId<'db>),
}

#[salsa::interned(debug)]
pub struct Ident {
    #[returns(ref)]
    pub text: String,
}
