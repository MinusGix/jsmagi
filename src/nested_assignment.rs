use smallvec::SmallVec;
use swc_common::Span;
use swc_ecma_ast::{AssignExpr, AssignOp, Expr, ExprStmt, ModuleItem, Stmt};
use swc_ecma_transforms_testing::test;
#[cfg(test)]
use swc_ecma_visit::as_folder;
use swc_ecma_visit::{noop_visit_mut_type, VisitMut, VisitMutWith};

/// Transform nest assignments like `a = b = c = d = (some literal)` to
/// `a = (some literal);\nb = (some literal);\nc = (some literal);\nd = (some literal);`
pub struct NestedAssignmentVisitor;

fn nested_assignment_converter(expr: Expr, span: Span) -> Vec<Expr> {
    let expr2 = expr.clone();
    let mut cexpr = &expr;

    let mut vars = SmallVec::<[_; 4]>::new();
    let mut val = None;
    while let Expr::Assign(expr) = &cexpr {
        vars.push(expr.left.clone());

        if expr.op != AssignOp::Assign {
            break;
        }

        // TODO: There's probably a bigger class that we could extract here
        if matches!(expr.right.as_ref(), Expr::Lit(_) | Expr::Ident(_)) {
            val = Some(expr.right.clone());
            break;
        }
        cexpr = &*expr.right;
    }

    if let Some(val) = val {
        vars.into_iter()
            .map(|var| {
                Expr::Assign(AssignExpr {
                    span,
                    left: var,
                    op: AssignOp::Assign,
                    right: val.clone(),
                })
            })
            .collect()
    } else {
        vec![expr2]
    }
}

impl VisitMut for NestedAssignmentVisitor {
    noop_visit_mut_type!();

    fn visit_mut_stmts(&mut self, stmts: &mut Vec<Stmt>) {
        let mut new_stmts = Vec::new();
        for stmt in stmts.drain(..) {
            match stmt {
                Stmt::Expr(ExprStmt { expr, span }) => {
                    let exprs = nested_assignment_converter(*expr, span);
                    new_stmts.extend(exprs.into_iter().map(|expr| {
                        Stmt::Expr(ExprStmt {
                            expr: Box::new(expr),
                            span,
                        })
                    }));
                }
                _ => new_stmts.push(stmt),
            }
        }
        *stmts = new_stmts;

        stmts.visit_mut_children_with(self);
    }

    fn visit_mut_module_items(&mut self, items: &mut Vec<ModuleItem>) {
        let mut new_items = Vec::new();
        for item in items.drain(..) {
            match item {
                ModuleItem::Stmt(Stmt::Expr(ExprStmt { expr, span })) => {
                    let exprs = nested_assignment_converter(*expr, span);
                    new_items.extend(exprs.into_iter().map(|expr| {
                        ModuleItem::Stmt(Stmt::Expr(ExprStmt {
                            expr: Box::new(expr),
                            span,
                        }))
                    }));
                }
                _ => new_items.push(item),
            }
        }
        *items = new_items;

        items.visit_mut_children_with(self);
    }
}

test!(
    Default::default(),
    |_| as_folder(NestedAssignmentVisitor),
    nested_assignment_sanity,
    "a = 1",
    "a = 1;"
);

test!(
    Default::default(),
    |_| as_folder(NestedAssignmentVisitor),
    nested_assignment,
    "a = b = c = d = 1",
    "a = 1;\nb = 1;\nc = 1;\nd = 1;"
);

test!(
    Default::default(),
    |_| as_folder(NestedAssignmentVisitor),
    nested_assignment2,
    "function abc() { a = b = c = d = 1 }",
    "function abc () { a = 1;\nb = 1;\nc = 1;\nd = 1; }"
);

test!(
    Default::default(),
    |_| as_folder(NestedAssignmentVisitor),
    nested_assignment3,
    "a = b = 1",
    "a = 1;\nb = 1;"
);

test!(
    Default::default(),
    |_| as_folder(NestedAssignmentVisitor),
    nested_assignment4,
    "t.a = t.b = t.c = t.d = 4",
    "t.a = 4;\nt.b = 4;\nt.c = 4;\nt.d = 4;"
);

test!(
    Default::default(),
    |_| as_folder(NestedAssignmentVisitor),
    nested_assignment5,
    "function abc() { t.a = t.b = t.c = t.d = 4; }",
    "function abc() { t.a = 4;\nt.b = 4;\nt.c = 4;\nt.d = 4; }"
);

test!(
    Default::default(),
    |_| as_folder(NestedAssignmentVisitor),
    nested_assignment6,
    "t.a = t.b = t.c = undefined;",
    "t.a = undefined;\nt.b = undefined;\nt.c = undefined;"
);
