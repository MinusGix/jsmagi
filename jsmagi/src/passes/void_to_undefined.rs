use swc_ecma_ast::{Expr, UnaryOp};
use swc_ecma_transforms_testing::test;
#[cfg(test)]
use swc_ecma_visit::as_folder;
use swc_ecma_visit::{noop_visit_mut_type, VisitMut, VisitMutWith};

use crate::{util::make_undefined, FromMagiConfig, MagiConfig};

/// Convert `void 0` to `undefined`  
/// Minifiers convert the statements because `void 0` is very slightly shorter, however it is less natural to read.
pub struct VoidToUndefinedVisitor;
impl FromMagiConfig for VoidToUndefinedVisitor {
    fn from_config(_conf: &MagiConfig) -> Self {
        Self
    }
}

impl VisitMut for VoidToUndefinedVisitor {
    noop_visit_mut_type!();

    fn visit_mut_expr(&mut self, expr: &mut Expr) {
        if let Expr::Unary(unary) = expr {
            if unary.op == UnaryOp::Void {
                // We assume that `void (some literal)` is always `undefined` with no side effects
                // TODO: There's a larger class of things that are always `undefined` that we could handle here, but this covers the common case
                if let Expr::Lit(_) = &*unary.arg {
                    *expr = make_undefined(unary.span);
                }
            }
        }

        expr.visit_mut_children_with(self);
    }
}

test!(
    Default::default(),
    |_| as_folder(VoidToUndefinedVisitor),
    void_0,
    "void 0" // "undefined"
);

test!(
    Default::default(),
    |_| as_folder(VoidToUndefinedVisitor),
    void_0_in_expr,
    "void 0 + 1" // "undefined + 1"
);

test!(
    Default::default(),
    |_| as_folder(VoidToUndefinedVisitor),
    void_0_in_expr_2,
    "1 + void 0" // "1 + undefined"
);

// test!(
//     Default::default(),
//     |_| as_folder(VoidToUndefinedVisitor),
//     void_0_in_expr_3,
//     "void (0 + 2)",
//     "undefined"
// );

test!(
    Default::default(),
    |_| as_folder(VoidToUndefinedVisitor),
    void_0_in_expr_4,
    "void console.log('hi')" // "void console.log('hi')"
);
