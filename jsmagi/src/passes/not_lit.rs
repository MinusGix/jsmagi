use swc_ecma_ast::{Bool, Expr, Lit, UnaryOp};
use swc_ecma_transforms_testing::test;
#[cfg(test)]
use swc_ecma_visit::as_folder;
use swc_ecma_visit::{noop_visit_mut_type, VisitMut, VisitMutWith};

use crate::{FromMagiConfig, MagiConfig};

pub struct NotLitVisitor;
impl FromMagiConfig for NotLitVisitor {
    fn from_config(_conf: &MagiConfig) -> Self {
        Self
    }
}

fn replace_not_lit(expr: &mut Expr) -> Option<()> {
    let unary = expr.as_unary()?;
    if unary.op != UnaryOp::Bang {
        return None;
    }

    let lit = unary.arg.as_lit()?;
    let Lit::Num(v) = lit else {
        return None;
    };

    let value = if v.value == 0.0 { true } else { false };

    *expr = Expr::Lit(Lit::Bool(Bool {
        value,
        span: unary.span,
    }));

    Some(())
}

impl VisitMut for NotLitVisitor {
    noop_visit_mut_type!();

    fn visit_mut_expr(&mut self, expr: &mut Expr) {
        replace_not_lit(expr);

        expr.visit_mut_children_with(self);
    }
}

test!(
    Default::default(),
    |_| as_folder(NotLitVisitor),
    not_lit,
    "!0" // "true"
);

test!(
    Default::default(),
    |_| as_folder(NotLitVisitor),
    not_lit1,
    "!1" // "false"
);

test!(
    Default::default(),
    |_| as_folder(NotLitVisitor),
    not_lit2,
    "!2" // "false"
);

test!(
    Default::default(),
    |_| as_folder(NotLitVisitor),
    not_lit3,
    "!'asdf'" // "!'asdf'"
);
