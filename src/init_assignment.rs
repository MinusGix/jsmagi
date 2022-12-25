use smallvec::{smallvec, SmallVec};
use swc_ecma_ast::{
    AssignExpr, AssignOp, BinExpr, BinaryOp, Expr, ExprStmt, MemberExpr, ModuleItem, Pat,
    PatOrExpr, Stmt,
};
use swc_ecma_transforms_testing::test;
#[cfg(test)]
use swc_ecma_visit::as_folder;
use swc_ecma_visit::{noop_visit_mut_type, VisitMut, VisitMutWith};

use crate::util::replace_entries;

/// `(c = n || (n = {})).thing = 'hi'` into
/// `n = n || {}; c = n; c.thing = 'hi'`
pub fn replace_init_assignment(root_stmt: &Stmt) -> Option<SmallVec<[Stmt; 3]>> {
    // (c = n || (n = {})).thing = 'hi'
    let Stmt::Expr(root_expr) = root_stmt else { return None };
    let Expr::Assign(root_assign) = root_expr.expr.as_ref() else { return None };
    // We only care about `=`, but supporting `+=` and other operators is possible, but only really beneficial if the default value has fields that are used
    if root_assign.op != AssignOp::Assign {
        return None;
    }

    // -> `(c = n || (n = {})).thing`
    let PatOrExpr::Pat(obj_left) = &root_assign.left else { return  None };
    let Pat::Expr(obj_left) = obj_left.as_ref() else { return  None };
    let Expr::Member(obj_left) = obj_left.as_ref() else { return  None };

    // -> `c = n || (n = {})`
    let Expr::Paren(inner_obj) = obj_left.obj.as_ref() else { return  None };
    let Expr::Assign(inner_obj) = &*inner_obj.expr else { return  None };

    // `c`. We only allow idents, for simplicity
    let PatOrExpr::Pat(c_obj) = & inner_obj.left else { return  None };
    let Pat::Ident(c_obj) = c_obj.as_ref() else { return  None };

    // `n || (n = {})`
    let Expr::Bin(default_expr) = inner_obj.right.as_ref() else { return  None };
    if default_expr.op != BinaryOp::LogicalOr {
        return None;
    }

    // `n`
    let Expr::Ident(n_obj) = default_expr.left.as_ref() else { return  None };

    // TODO: support version without parens?
    // `(n = {})`
    let Expr::Paren(n_assign) = default_expr.right.as_ref() else { return  None };
    let Expr::Assign(n_assign) = n_assign.expr.as_ref() else { return  None };
    // Doesn't make sense to support other assignment operators here
    if n_assign.op != AssignOp::Assign {
        return None;
    }

    // `n`
    let PatOrExpr::Pat(n_assign_obj) = &n_assign.left else { return  None };
    let Pat::Ident(n_assign_obj) = n_assign_obj.as_ref() else { return  None };

    // `{}`
    let Expr::Object(right_paren_assign_right) = n_assign.right.as_ref() else { return  None };
    // TODO: We can do better than this
    if !right_paren_assign_right.props.is_empty() {
        return None;
    }

    // If they don't match, then this isn't what we're looking for..
    if n_obj.sym != n_assign_obj.sym {
        return None;
    }

    // TODO: any more verifications we need to make?

    // `n = n || {};`
    let n_init = Box::new(Expr::Assign(AssignExpr {
        span: root_assign.span,
        op: AssignOp::Assign,
        left: PatOrExpr::Pat(Box::new(Pat::Ident(n_assign_obj.clone()))),
        right: Box::new(Expr::Bin(BinExpr {
            span: default_expr.span,
            op: BinaryOp::LogicalOr,
            left: Box::new(Expr::Ident(n_obj.clone())),
            right: Box::new(Expr::Object(right_paren_assign_right.clone())),
        })),
    }));

    // `c = n;`
    let c_init = Box::new(Expr::Assign(AssignExpr {
        span: root_assign.span,
        op: AssignOp::Assign,
        left: PatOrExpr::Pat(Box::new(Pat::Ident(c_obj.clone()))),
        right: Box::new(Expr::Ident(n_obj.clone())),
    }));

    // `c.thing = 'hi'`
    let c_field_init = Box::new(Expr::Assign(AssignExpr {
        span: root_assign.span,
        op: AssignOp::Assign,
        left: PatOrExpr::Expr(Box::new(Expr::Member(MemberExpr {
            span: obj_left.span,
            prop: obj_left.prop.clone(),
            obj: Box::new(Expr::Ident(c_obj.id.clone())),
        }))),
        right: root_assign.right.clone(),
    }));

    Some(smallvec![
        Stmt::Expr(ExprStmt {
            span: root_expr.span,
            expr: n_init,
        }),
        Stmt::Expr(ExprStmt {
            span: root_expr.span,
            expr: c_init,
        }),
        Stmt::Expr(ExprStmt {
            span: root_expr.span,
            expr: c_field_init,
        }),
    ])
}

/// `(c = n || (n = {})).thing = 'hi'` into
/// `n = n || {}; c = n; c.thing = 'hi'`
pub struct InitAssignmentVisitor;

impl VisitMut for InitAssignmentVisitor {
    noop_visit_mut_type!();

    fn visit_mut_stmts(&mut self, stmts: &mut Vec<Stmt>) {
        replace_entries(stmts, replace_init_assignment);

        stmts.visit_mut_children_with(self);
    }

    fn visit_mut_module_items(&mut self, n: &mut Vec<ModuleItem>) {
        replace_entries(n, |x| {
            if let ModuleItem::Stmt(x) = x {
                replace_init_assignment(x)
            } else {
                None
            }
        });

        n.visit_mut_children_with(self);
    }
}

test!(
    Default::default(),
    |_| as_folder(InitAssignmentVisitor),
    single_variable,
    r#"let n;"#,
    "let n;"
);

test!(
    Default::default(),
    |_| as_folder(InitAssignmentVisitor),
    weird_assign,
    // TODO: We can do better than this in some cases.
    // It is common for it to assign no value to `c`, and sometimes for `n` to be unused after initialization
    // but we'll need more complicated detection
    "(c = n || (n = {})).thing = 'hi'",
    "n = n || {};\nc = n;\nc.thing = 'hi'"
);
