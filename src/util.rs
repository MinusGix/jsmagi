use std::collections::HashMap;

use swc_atoms::js_word;
use swc_common::{EqIgnoreSpan, Span, SyntaxContext};
use swc_ecma_ast::{
    AssignExpr, AssignOp, BinExpr, BinaryOp, BindingIdent, Expr, Id, Ident, MemberExpr, MemberProp,
    ObjectLit, ParenExpr, Pat, PatOrExpr,
};
use swc_ecma_visit::{noop_visit_mut_type, VisitMut};

pub fn make_undefined(span: Span) -> Expr {
    Expr::Ident(Ident::new(js_word!("undefined"), span))
}

/// Remove the parens from an expression, if they exist
pub fn unwrap_parens(expr: &Expr) -> &Expr {
    if let Expr::Paren(paren) = expr {
        unwrap_parens(&paren.expr)
    } else {
        expr
    }
}

/// Remove the parens from an expression, if they exist
pub fn unwrap_parens_mut(expr: &mut Expr) -> &mut Expr {
    if let Expr::Paren(paren) = expr {
        unwrap_parens_mut(&mut paren.expr)
    } else {
        expr
    }
}

pub fn make_empty_object(span: Span) -> Expr {
    Expr::Object(ObjectLit {
        span,
        props: vec![],
    })
}

pub fn extract_expr_from_pat_or_expr(pat_or_expr: &PatOrExpr) -> Option<&Expr> {
    Some(match pat_or_expr {
        PatOrExpr::Expr(left) => left,
        PatOrExpr::Pat(left) => {
            let Pat::Expr(left) = left.as_ref() else { return None; };
            left
        }
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
pub fn extract_or_assign_initializer(expr: &Expr) -> Option<(Ident, NiceAccess)> {
    let Expr::Assign(assign) = expr else { return None; };

    // Get the identifier on the left side of the assignment
    let PatOrExpr::Pat(left) = &assign.left else { return None; };

    // TODO: We could support more complex expressions on the left side
    let Pat::Ident(left_ident) = left.as_ref() else { return None; };

    // Get the right side of the assignment
    let right = unwrap_parens(&assign.right);

    // Check if the right side is of the form `x || (x = {})`
    let right_access = extract_or_initializer(right)?;

    // TODO: Should we do anything special if it is of the weird form `x = x || (x = {})`?

    Some((left_ident.id.clone(), right_access))
}

/// Check if the expression is of the form `x || (x = {})`, returning the expr
pub fn extract_or_initializer(expr: &Expr) -> Option<NiceAccess> {
    let Expr::Bin(bin) = expr else { return None; };

    if bin.op != BinaryOp::LogicalOr {
        return None;
    }

    let right = unwrap_parens(&bin.right);
    let Expr::Assign(assign) = right else { return None; };
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
    let Expr::Assign(assign) = expr else {return None;};

    if assign.op == AssignOp::Assign {
        Some(assign)
    } else {
        None
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
