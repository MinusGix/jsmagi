use swc_ecma_ast::{Decl, ModuleItem, Stmt, VarDecl};
use swc_ecma_transforms_testing::test;
#[cfg(test)]
use swc_ecma_visit::as_folder;
use swc_ecma_visit::{noop_visit_mut_type, VisitMut, VisitMutWith};

use crate::{FromMagiConfig, MagiConfig};

/// `let a, b, c;` => `let a; let b; let c;`
pub struct VarDeclExpand;
impl FromMagiConfig for VarDeclExpand {
    fn from_config(_conf: &MagiConfig) -> Self {
        Self
    }
}

impl VisitMut for VarDeclExpand {
    noop_visit_mut_type!();

    fn visit_mut_stmts(&mut self, stmts: &mut Vec<Stmt>) {
        let mut new_stmts = Vec::new();
        for stmt in stmts.drain(..) {
            match stmt {
                Stmt::Decl(decl) => match decl {
                    Decl::Var(var) => {
                        for decl in var.decls {
                            new_stmts.push(Stmt::Decl(Decl::Var(Box::new(VarDecl {
                                span: var.span,
                                kind: var.kind,
                                declare: var.declare,
                                decls: vec![decl],
                            }))));
                        }
                    }
                    _ => new_stmts.push(Stmt::Decl(decl)),
                },
                _ => new_stmts.push(stmt),
            }
        }
        *stmts = new_stmts;

        stmts.visit_mut_children_with(self);
    }

    // The same as the visit_mut_stmts, but for module items
    fn visit_mut_module_items(&mut self, items: &mut Vec<ModuleItem>) {
        let mut new_items = Vec::new();
        for item in items.drain(..) {
            match item {
                ModuleItem::Stmt(stmt) => match stmt {
                    Stmt::Decl(decl) => match decl {
                        Decl::Var(var) => {
                            for decl in var.decls {
                                new_items.push(ModuleItem::Stmt(Stmt::Decl(Decl::Var(Box::new(
                                    VarDecl {
                                        span: var.span,
                                        kind: var.kind,
                                        declare: var.declare,
                                        decls: vec![decl],
                                    },
                                )))));
                            }
                        }
                        _ => new_items.push(ModuleItem::Stmt(Stmt::Decl(decl))),
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

// TODO: We can do this by visiting stmt and matching for decl?
// and then somehow replacing it? Perhaps inserting a block, and then have another transform to get
// rid of useless blocks?
// TODO: It seems like `Seq` represents this
test!(
    Default::default(),
    |_| as_folder(VarDeclExpand),
    multivariable,
    r#"let n,o,b,c,d,e;"# // "let n;\nlet o;\nlet b;\nlet c;\nlet d;\nlet e;"
);
