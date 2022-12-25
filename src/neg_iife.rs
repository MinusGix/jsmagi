use swc_ecma_ast::{CallExpr, Callee, Expr, ExprStmt, Stmt, UnaryOp};
use swc_ecma_transforms_testing::test;
#[cfg(test)]
use swc_ecma_visit::as_folder;
use swc_ecma_visit::{noop_visit_mut_type, VisitMut, VisitMutWith};

/// `!function (x) { ... }(x)` expr stmt => `(function () {})()`  
/// May not be eval-safe
pub struct NegIifeVisitor;

impl VisitMut for NegIifeVisitor {
    noop_visit_mut_type!();

    fn visit_mut_stmt(&mut self, stmt: &mut Stmt) {
        if let Stmt::Expr(expr) = stmt {
            if let Expr::Unary(unary) = &mut *expr.expr {
                if unary.op == UnaryOp::Bang {
                    if let Expr::Call(call) = &mut *unary.arg {
                        if let Callee::Expr(fn_expr) = &mut call.callee {
                            if let Expr::Fn(fn_decl) = &mut **fn_expr {
                                *stmt = Stmt::Expr(ExprStmt {
                                    span: expr.span,
                                    expr: Box::new(Expr::Call(CallExpr {
                                        span: expr.span,
                                        callee: Callee::Expr(Box::new(Expr::Fn(fn_decl.clone()))),
                                        args: call.args.clone(),
                                        type_args: None,
                                    })),
                                });
                            }
                        }
                    }
                }
            }
        }

        stmt.visit_mut_children_with(self);
    }
}

test!(
    Default::default(),
    |_| as_folder(NegIifeVisitor),
    neg_iife,
    "!function (x) { alert('hi') }(x)",
    "(function (x) { alert('hi') })(x)"
);

// TODO: We could optimize some basic cases where they don't return a value from a function
test!(
    Default::default(),
    |_| as_folder(NegIifeVisitor),
    neg_iife_sanity,
    "let j = !function (x) { return x }(x);",
    "let j = !function (x) { return x }(x);"
);
