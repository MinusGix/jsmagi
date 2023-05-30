use swc_ecma_ast::{CallExpr, Callee, Expr, ExprStmt, Stmt, UnaryOp};
use swc_ecma_transforms_testing::test;
#[cfg(test)]
use swc_ecma_visit::as_folder;
use swc_ecma_visit::{noop_visit_mut_type, VisitMut, VisitMutWith};

use crate::{FromMagiConfig, MagiConfig};

/// `!function (x) { ... }(x)` expr stmt => `(function () {})()`  
/// May not be eval-safe
pub struct NotIifeVisitor;
impl FromMagiConfig for NotIifeVisitor {
    fn from_config(_conf: &MagiConfig) -> Self {
        Self
    }
}

fn replace_not_iife(stmt: &mut Stmt) -> Option<()> {
    let expr = stmt.as_expr()?;
    let unary = expr.expr.as_unary()?;
    if unary.op != UnaryOp::Bang {
        return None;
    }

    let call = unary.arg.as_call()?;
    let fn_expr = call.callee.as_expr()?.as_fn_expr()?;

    *stmt = ExprStmt {
        span: expr.span,
        // TODO: This could theoretically just move the function expression
        expr: Box::new(Expr::Call(CallExpr {
            span: expr.span,
            callee: Callee::Expr(Box::new(Expr::Fn(fn_expr.clone()))),
            args: call.args.clone(),
            type_args: None,
        })),
    }
    .into();

    Some(())
}

impl VisitMut for NotIifeVisitor {
    noop_visit_mut_type!();

    fn visit_mut_stmt(&mut self, stmt: &mut Stmt) {
        replace_not_iife(stmt);

        stmt.visit_mut_children_with(self);
    }
}

test!(
    Default::default(),
    |_| as_folder(NotIifeVisitor),
    neg_iife,
    "!function (x) { alert('hi') }(x)",
    "(function (x) { alert('hi') })(x)"
);

// TODO: We could optimize some basic cases where they don't return a value from a function
test!(
    Default::default(),
    |_| as_folder(NotIifeVisitor),
    neg_iife_sanity,
    "let j = !function (x) { return x }(x);",
    "let j = !function (x) { return x }(x);"
);
