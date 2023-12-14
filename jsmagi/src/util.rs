use std::collections::HashMap;

use swc_atoms::js_word;
use swc_common::{pass::Either, EqIgnoreSpan, Span, SyntaxContext};
use swc_ecma_ast::{
    AssignExpr, AssignOp, BinExpr, BinaryOp, BindingIdent, Expr, Id, Ident, MemberExpr, MemberProp,
    ModuleItem, ObjectLit, ParenExpr, Pat, PatOrExpr, Stmt,
};
use swc_ecma_visit::{noop_visit_mut_type, VisitMut};

pub fn make_undefined(span: Span) -> Expr {
    Expr::Ident(Ident::new(js_word!("undefined"), span))
}

pub fn make_empty_object(span: Span) -> Expr {
    Expr::Object(ObjectLit {
        span,
        props: vec![],
    })
}

/// For nicely behaved accessors, like an identifier or `a.b` or `a['b']  
/// Note that this is over-eager, because of custom getter functions and the like.
/// But you'd need complicated analyses to find those properly, and it is bad coding style.
#[derive(Debug, Clone)]
pub enum NiceAccess {
    Ident(Ident),
    /// This is a nice member expression, so it won't have any side effects
    Member(MemberExpr),
}
impl NiceAccess {
    pub fn is_basically_equiv(&self, other: &NiceAccess) -> bool {
        match (self, other) {
            (NiceAccess::Ident(a), NiceAccess::Ident(b)) => a.to_id() == b.to_id(),
            (NiceAccess::Member(a), NiceAccess::Member(b)) => a.eq_ignore_span(&b),
            _ => false,
        }
    }
}
impl TryInto<Pat> for NiceAccess {
    type Error = ();

    fn try_into(self) -> Result<Pat, Self::Error> {
        match self {
            NiceAccess::Ident(ident) => Ok(Pat::Ident(BindingIdent {
                id: ident,
                type_ann: None,
            })),
            NiceAccess::Member(member) => Ok(Pat::Expr(Box::new(Expr::Member(member)))),
        }
    }
}
impl TryInto<PatOrExpr> for NiceAccess {
    type Error = ();

    fn try_into(self) -> Result<PatOrExpr, Self::Error> {
        match self {
            NiceAccess::Ident(ident) => Ok(PatOrExpr::Pat(Box::new(Pat::Ident(BindingIdent {
                id: ident,
                type_ann: None,
            })))),
            NiceAccess::Member(member) => Ok(PatOrExpr::Pat(Box::new(Pat::Expr(Box::new(
                Expr::Member(member),
            ))))),
        }
    }
}
impl TryInto<Expr> for NiceAccess {
    type Error = ();

    fn try_into(self) -> Result<Expr, Self::Error> {
        match self {
            NiceAccess::Ident(ident) => Ok(Expr::Ident(ident)),
            NiceAccess::Member(member) => Ok(Expr::Member(member)),
        }
    }
}
impl<'a> TryFrom<&'a Expr> for NiceAccess {
    type Error = ();

    fn try_from(expr: &'a Expr) -> Result<Self, Self::Error> {
        match expr {
            Expr::Ident(ident) => Ok(NiceAccess::Ident(ident.clone())),
            Expr::Member(member) => {
                // TODO: Check that obj is nice
                // let obj = member.obj.as_ref();
                match &member.prop {
                    MemberProp::Ident(_) => Ok(NiceAccess::Member(member.clone())),
                    // TODO: we can probably support this
                    MemberProp::PrivateName(_) => return Err(()),
                    // TODO: We should be able to do a check for if it is a simple-expression
                    MemberProp::Computed(_) => return Err(()),
                }
            }
            _ => Err(()),
        }
    }
}
impl<'a> TryFrom<&'a PatOrExpr> for NiceAccess {
    type Error = ();

    fn try_from(pat_or_expr: &'a PatOrExpr) -> Result<Self, Self::Error> {
        match pat_or_expr {
            PatOrExpr::Pat(pat) => match pat.as_ref() {
                Pat::Ident(ident) => Ok(NiceAccess::Ident(ident.id.clone())),
                Pat::Expr(expr) => Self::try_from(expr.as_ref()),
                _ => Err(()),
            },
            PatOrExpr::Expr(expr) => Self::try_from(expr.as_ref()),
        }
    }
}

/// Check if the expression is of the form `a = x || (x = {})` and return both identifiers  
/// Otherwise, check if the expression is of the form `a || (a = {})` and return the identifier
pub fn extract_or_initializer_with_assign(expr: &Expr) -> Option<(Option<Ident>, NiceAccess)> {
    extract_or_assign_initializer(expr)
        .map(|(a, b)| (Some(a), b))
        .or_else(|| Some((None, extract_or_initializer(expr)?)))
}

/// Check if the expression is of the form `a = x || (x = {})` and return both identifiers
pub fn extract_or_assign_initializer(expr: &Expr) -> Option<(Ident, NiceAccess)> {
    let assign = expr.as_assign()?;

    // Get the identifier on the left side of the assignment
    let left = assign.left.as_pat()?;

    // TODO: We could support more complex expressions on the left side
    let left_ident = left.as_ident()?;

    // Get the right side of the assignment
    let right = assign.right.unwrap_parens();

    // Check if the right side is of the form `x || (x = {})`
    let right_access = extract_or_initializer(right)?;

    // TODO: Should we do anything special if it is of the weird form `x = x || (x = {})`?

    Some((left_ident.id.clone(), right_access))
}

/// Check if the expression is of the form `x || (x = {})`, returning the expr
pub fn extract_or_initializer(expr: &Expr) -> Option<NiceAccess> {
    let bin = expr.as_bin()?;

    if bin.op != BinaryOp::LogicalOr {
        return None;
    }

    let assign = bin.right.unwrap_parens().as_assign()?;
    if assign.op != AssignOp::Assign {
        return None;
    }

    let right = &assign.left;
    let left = bin.left.as_ref();

    let left = NiceAccess::try_from(left).ok()?;
    let right = NiceAccess::try_from(right).ok()?;

    if left.is_basically_equiv(&right) {
        Some(left)
    } else {
        None
    }
}

/// Create an expression of the form `x || (x = {})` from an identifier
pub fn make_or_initializer(ident: Ident) -> Expr {
    Expr::Bin(BinExpr {
        span: ident.span,
        op: BinaryOp::LogicalOr,
        left: Box::new(Expr::Ident(ident.clone())),
        right: Box::new(Expr::Paren(ParenExpr {
            span: ident.span,
            expr: Box::new(Expr::Assign(AssignExpr {
                span: ident.span,
                left: PatOrExpr::Expr(Box::new(Expr::Ident(ident.clone()))),
                op: AssignOp::Assign,
                right: Box::new(make_empty_object(ident.span)),
            })),
        })),
    })
}

/// Get an `AssignExpr` if the expression is of the form `x = y`
pub fn get_assign_eq_expr(expr: &Expr) -> Option<&AssignExpr> {
    let assign = expr.as_assign()?;

    if assign.op == AssignOp::Assign {
        Some(assign)
    } else {
        None
    }
}

#[derive(Debug)]
pub enum Stmts<'a> {
    Stmts(&'a Vec<Stmt>),
    Module(&'a Vec<ModuleItem>),
}

impl<'a> Stmts<'a> {
    pub fn iter<'s: 'a>(&'s self) -> impl Iterator<Item = &'a Stmt> + 's {
        match self {
            Stmts::Stmts(stmts) => Either::Left(stmts.iter()),
            Stmts::Module(stmts) => Either::Right(stmts.iter().filter_map(ModuleItem::as_stmt)),
        }
    }
}

impl<'a> From<&'a Vec<Stmt>> for Stmts<'a> {
    fn from(stmts: &'a Vec<Stmt>) -> Self {
        Stmts::Stmts(stmts)
    }
}

impl<'a> From<&'a Vec<ModuleItem>> for Stmts<'a> {
    fn from(stmts: &'a Vec<ModuleItem>) -> Self {
        Stmts::Module(stmts)
    }
}

#[derive(Debug)]
pub enum StmtsMut<'a> {
    Stmts(&'a mut Vec<Stmt>),
    Module(&'a mut Vec<ModuleItem>),
}
impl<'a> StmtsMut<'a> {
    // is this lifetime sane?
    pub fn iter<'s>(&'s self) -> impl Iterator<Item = &'s Stmt> + DoubleEndedIterator
    where
        'a: 's,
    {
        match self {
            StmtsMut::Stmts(stmts) => Either::Left(stmts.iter()),
            StmtsMut::Module(stmts) => Either::Right(stmts.iter().filter_map(ModuleItem::as_stmt)),
        }
    }

    pub fn iter_mut<'s>(&'s mut self) -> impl Iterator<Item = &'s mut Stmt> + DoubleEndedIterator
    where
        'a: 's,
    {
        match self {
            StmtsMut::Stmts(stmts) => Either::Left(stmts.iter_mut()),
            StmtsMut::Module(stmts) => {
                Either::Right(stmts.iter_mut().filter_map(ModuleItem::as_mut_stmt))
            }
        }
    }

    /// Iterate with indices into the underlying vector.
    pub fn iter_idx<'s>(&'s self) -> impl Iterator<Item = (usize, &'s Stmt)>
    where
        'a: 's,
    {
        match self {
            StmtsMut::Stmts(stmts) => Either::Left(stmts.iter().enumerate()),
            StmtsMut::Module(stmts) => Either::Right(
                stmts
                    .iter()
                    .map(ModuleItem::as_stmt)
                    .enumerate()
                    .filter_map(|(i, s)| s.map(|s| (i, s))),
            ),
        }
    }

    /// Iterate with indices into the underlying vector.
    pub fn iter_mut_idx<'s>(&'s mut self) -> impl Iterator<Item = (usize, &'s mut Stmt)>
    where
        'a: 's,
    {
        match self {
            StmtsMut::Stmts(stmts) => Either::Left(stmts.iter_mut().enumerate()),
            StmtsMut::Module(stmts) => Either::Right(
                stmts
                    .iter_mut()
                    .map(ModuleItem::as_mut_stmt)
                    .enumerate()
                    .filter_map(|(i, s)| s.map(|s| (i, s))),
            ),
        }
    }

    pub fn push(&mut self, stmt: Stmt) {
        match self {
            StmtsMut::Stmts(stmts) => stmts.push(stmt),
            StmtsMut::Module(stmts) => stmts.push(ModuleItem::Stmt(stmt)),
        }
    }

    /// Insert a statement at the given index.
    ///
    /// # Panics
    /// If `idx > len`
    pub fn insert(&mut self, idx: usize, stmt: Stmt) {
        match self {
            StmtsMut::Stmts(stmts) => stmts.insert(idx, stmt),
            StmtsMut::Module(stmts) => stmts.insert(idx, ModuleItem::Stmt(stmt)),
        }
    }
}

impl<'a> From<&'a mut Vec<Stmt>> for StmtsMut<'a> {
    fn from(stmts: &'a mut Vec<Stmt>) -> Self {
        StmtsMut::Stmts(stmts)
    }
}
impl<'a> From<&'a mut Vec<ModuleItem>> for StmtsMut<'a> {
    fn from(stmts: &'a mut Vec<ModuleItem>) -> Self {
        StmtsMut::Module(stmts)
    }
}

pub fn replace_entries<T, J, I, F>(data: &'_ mut Vec<T>, f: F)
where
    T: 'static,
    J: Into<T> + 'static,
    I: IntoIterator<Item = J> + 'static,
    <I as IntoIterator>::IntoIter: DoubleEndedIterator,
    F: Fn(&T) -> Option<I>,
{
    let mut result: Vec<(usize, I)> = Vec::new();
    {
        for (i, entry) in data.iter().enumerate() {
            if let Some(values) = f(entry) {
                result.push((i, values));
            }
        }
    }

    let mut offset = 0;
    for (i, values) in result {
        let values = values.into_iter().map(Into::into);
        // replace the single entry at `i` with the new values, without using the splice function

        let i = i + offset;
        data.remove(i);
        for value in values.rev() {
            data.insert(i, value);
            offset += 1;
        }
    }
}

// The remapper code is from SWC, and so is under their License.
/// Variable remapper
///
/// - Used for evaluating IIFEs
pub(crate) struct Remapper {
    pub vars: HashMap<Id, SyntaxContext>,
}

impl VisitMut for Remapper {
    noop_visit_mut_type!();

    fn visit_mut_ident(&mut self, i: &mut Ident) {
        if let Some(new_ctxt) = self.vars.get(&i.to_id()).copied() {
            i.span.ctxt = new_ctxt;
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_replace_entries() {
        let mut data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        super::replace_entries(
            &mut data,
            |x| {
                if *x == 3 {
                    Some(vec![1, 2, 3])
                } else {
                    None
                }
            },
        );
        assert_eq!(data, vec![1, 2, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    }
}
