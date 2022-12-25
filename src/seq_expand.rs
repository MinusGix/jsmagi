use swc_ecma_ast::{Expr, ExprStmt, ModuleItem, Stmt};
use swc_ecma_transforms_testing::test;
#[cfg(test)]
use swc_ecma_visit::as_folder;
use swc_ecma_visit::{noop_visit_mut_type, VisitMut, VisitMutWith};

/// Converts `a, b, c` statements into `a; b; c;`
pub struct SeqExpandVisitor;

impl VisitMut for SeqExpandVisitor {
    noop_visit_mut_type!();

    fn visit_mut_stmts(&mut self, stmts: &mut Vec<Stmt>) {
        let mut new_stmts = Vec::new();
        for stmt in stmts.drain(..) {
            match stmt {
                Stmt::Expr(ExprStmt { span, expr }) => match *expr {
                    Expr::Seq(seq) => {
                        for expr in seq.exprs {
                            new_stmts.push(Stmt::Expr(ExprStmt { span, expr }));
                        }
                    }
                    _ => new_stmts.push(Stmt::Expr(ExprStmt { span, expr })),
                },
                _ => new_stmts.push(stmt),
            }
        }
        *stmts = new_stmts;

        stmts.visit_mut_children_with(self);
    }

    // The same as visit_mut_stmts but for the `visit_mut_module_items` method
    fn visit_mut_module_items(&mut self, items: &mut Vec<ModuleItem>) {
        let mut new_items = Vec::new();
        for item in items.drain(..) {
            match item {
                ModuleItem::Stmt(stmt) => match stmt {
                    Stmt::Expr(ExprStmt { span, expr }) => match *expr {
                        Expr::Seq(seq) => {
                            for expr in seq.exprs {
                                new_items
                                    .push(ModuleItem::Stmt(Stmt::Expr(ExprStmt { span, expr })));
                            }
                        }
                        _ => new_items.push(ModuleItem::Stmt(Stmt::Expr(ExprStmt { span, expr }))),
                    },
                    _ => new_items.push(ModuleItem::Stmt(stmt)),
                },
                _ => new_items.push(item),
            }
        }
        *items = new_items;

        items.visit_mut_children_with(self);
    }
}

test!(
    Default::default(),
    |_| as_folder(SeqExpandVisitor),
    seq,
    "a, b, c",
    "a; b; c;"
);

test!(
    Default::default(),
    |_| as_folder(SeqExpandVisitor),
    seq2,
    "Object.defineProperty(), t.a = t.b = t.c = t.d = t.e",
    "Object.defineProperty(); t.a = t.b = t.c = t.d = t.e;"
);

test!(
    Default::default(),
    |_| as_folder(SeqExpandVisitor),
    seq3,
    "function a () { Object.defineProperty(), t.a = t.b = t.c = t.d = t.e }",
    "function a () { Object.defineProperty(); t.a = t.b = t.c = t.d = t.e; }"
);
