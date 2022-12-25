use std::collections::HashMap;

use swc_atoms::{js_word, JsWord};

use swc_ecma_ast::{
    BlockStmtOrExpr, Callee, Expr, ExprOrSpread, ExprStmt, Lit, MemberProp, Pat, Stmt,
};
use swc_ecma_transforms_base::rename::rename;
use swc_ecma_transforms_testing::test;
#[cfg(test)]
use swc_ecma_visit::as_folder;
use swc_ecma_visit::{noop_visit_mut_type, VisitMut, VisitMutWith};

/// If the variable has `__esModule` defined on it with `Object.defineProperty` then assume that it is an es module and rename it to `exports`.  
///
/// This may be overly aggressive, and should maybe only be used if you have reason to believe it using this specific method of doing es modules
pub struct EsModuleRenameVisitor;

fn visit_expr(expr: &mut Expr) {
    let Expr::Arrow(arrow) = &expr else { return };

    let BlockStmtOrExpr::BlockStmt(block) = &arrow.body else { return };

    let mut name = None;
    for stmt in &block.stmts {
        let Stmt::Expr(ExprStmt { expr, .. }) = stmt else { continue };

        let Expr::Call(call) = expr.as_ref() else { continue };

        let Callee::Expr(member) = &call.callee else { continue };
        let Expr::Member(member) = member.as_ref() else { continue };

        let Expr::Ident(ident) = member.obj.as_ref() else { continue };

        if ident.sym != js_word!("Object") {
            continue;
        }

        // TODO: Check if this is a computed property?
        let MemberProp::Ident(ident) = &member.prop else { continue };

        if ident.sym != JsWord::from("defineProperty") {
            continue;
        }

        // check that the second parameter is "__esModule"
        {
            let ExprOrSpread { expr, .. } = call.args.get(1).unwrap();

            let Expr::Lit(lit) = expr.as_ref() else { continue };

            let Lit::Str(text) = &lit else { continue };

            if text.value != JsWord::from("__esModule") {
                continue;
            }
        }

        {
            let ExprOrSpread { expr, .. } = call.args.get(0).unwrap();
            let Expr::Ident(ident) = expr.as_ref() else { continue };

            name = Some(ident.clone());
        }
    }

    if let Some(name) = name {
        for param in &arrow.params {
            let Pat::Ident(ident) = param else { continue };

            if name.to_id() == ident.id.to_id() {
                let mut rename_map = HashMap::default();
                // TODO: may need to make sure that this doesn't collide with any other variables
                let new_id: JsWord = "exports".to_string().into();
                rename_map.insert(name.to_id(), new_id);
                let mut ren = rename(&rename_map);
                expr.visit_mut_with(&mut ren);
                break;
            }
        }
    }
}

// TODO: We can be smarter than this
impl VisitMut for EsModuleRenameVisitor {
    noop_visit_mut_type!();

    fn visit_mut_expr(&mut self, expr: &mut Expr) {
        visit_expr(expr);

        expr.visit_mut_children_with(self);
    }

    // fn visit_mut_stmt(&mut self, stmt: &mut Stmt) {
    //     println!("Visit Stmt {:?}", stmt);
    //     let Stmt::Expr(ExprStmt { expr, .. }) = stmt else { return };

    //     visit_expr(expr);
    // }

    // fn visit_mut_module_item(&mut self, item: &mut ModuleItem) {
    //     println!("Visit Item {:?}", item);
    //     let ModuleItem::Stmt(Stmt::Expr(ExprStmt { expr, .. })) = item else { return };

    //     visit_expr(expr);
    // }
}

test!(
    Default::default(),
    |_| as_folder(EsModuleRenameVisitor),
    rename1,
    "(e, t, n) => { Object.defineProperty(t, \"__esModule\", { value: true }); t.x = 5; }",
    "(e, exports, n) => { Object.defineProperty(exports, \"__esModule\", { value: true }); exports.x = 5; }"
);

test!(
    Default::default(),
    |_| as_folder(EsModuleRenameVisitor),
    rename2,
    "var e = { 38: (e, t, n) => { Object.defineProperty(t, \"__esModule\", { value: true }); t.x = 5; } };",
    "var e = { 38: (e, exports, n) => { Object.defineProperty(exports, \"__esModule\", { value: true }); exports.x = 5; } };"
);
