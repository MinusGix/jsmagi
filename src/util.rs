use std::collections::HashMap;

use swc_atoms::js_word;
use swc_common::{Span, SyntaxContext};
use swc_ecma_ast::{
    AssignExpr, AssignOp, BinExpr, BinaryOp, Expr, Id, Ident, ObjectLit, ParenExpr, Pat, PatOrExpr,
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

/// Check if the expression is of the form `a = x || (x = {})` and return both identifiers
pub fn extract_or_assign_initializer(expr: &Expr) -> Option<(Ident, Ident)> {
    let Expr::Assign(assign) = expr else { return None; };

    // Get the identifier on the left side of the assignment
    let PatOrExpr::Pat(left) = &assign.left else { return None; };

    // TODO: We could support more complex expressions on the left side
    let Pat::Ident(left_ident) = left.as_ref() else { return None; };

    // Get the right side of the assignment
    let right = unwrap_parens(&assign.right);

    // Check if the right side is of the form `x || (x = {})`
    let right_ident = extract_or_initializer(right)?;

    // TODO: Should we do anything special if it is of the weird form `x = x || (x = {})`?

    Some((left_ident.id.clone(), right_ident))
}

/// Check if the expression is of the form `x || (x = {})`, returning the identifier
pub fn extract_or_initializer(expr: &Expr) -> Option<Ident> {
    let Expr::Bin(bin) = expr else { return None; };

    if bin.op != BinaryOp::LogicalOr {
        return None;
    }

    let right = unwrap_parens(&bin.right);
    let Expr::Assign(assign) = right else { return None; };

    // let PatOrExpr::Expr(left) = &assign.left else { return None; };
    let PatOrExpr::Pat(left) = &assign.left else { return None; };
    let Pat::Ident(left_ident) = left.as_ref() else { return None; };

    // let Expr::Ident(left_ident) = left.as_ref() else { return None; };

    let Expr::Ident(right_ident) = bin.left.as_ref() else { return None; };

    if left_ident.sym == right_ident.sym {
        Some(left_ident.id.clone())
    } else {
        // They weren't equal, thus it wasn't what we were looking for
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
