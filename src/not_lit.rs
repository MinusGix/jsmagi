use swc_ecma_ast::{Bool, Expr, Lit, UnaryOp};
use swc_ecma_transforms_testing::test;
#[cfg(test)]
use swc_ecma_visit::as_folder;
use swc_ecma_visit::{noop_visit_mut_type, VisitMut, VisitMutWith};

pub struct NotLitVisitor;

impl VisitMut for NotLitVisitor {
    noop_visit_mut_type!();

    fn visit_mut_expr(&mut self, expr: &mut Expr) {
        if let Expr::Unary(unary) = expr {
            if unary.op == UnaryOp::Bang {
                if let Expr::Lit(lit) = &*unary.arg {
                    if let Lit::Num(v) = lit {
                        if v.value == 0.0 {
                            *expr = Expr::Lit(Lit::Bool(Bool {
                                value: true,
                                span: unary.span,
                            }));
                        } else {
                            *expr = Expr::Lit(Lit::Bool(Bool {
                                value: false,
                                span: unary.span,
                            }));
                        }
                    }
                }
            }
        }

        expr.visit_mut_children_with(self);
    }
}

test!(
    Default::default(),
    |_| as_folder(NotLitVisitor),
    not_lit,
    "!0",
    "true"
);

test!(
    Default::default(),
    |_| as_folder(NotLitVisitor),
    not_lit1,
    "!1",
    "false"
);

test!(
    Default::default(),
    |_| as_folder(NotLitVisitor),
    not_lit2,
    "!2",
    "false"
);

test!(
    Default::default(),
    |_| as_folder(NotLitVisitor),
    not_lit3,
    "!'asdf'",
    "!'asdf'"
);
