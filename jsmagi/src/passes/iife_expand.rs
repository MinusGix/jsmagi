use std::collections::HashMap;

use crate::{
    rename::RenameIdentPass,
    util::{
        extract_or_initializer_with_assign, get_assign_eq_expr, make_empty_object, make_undefined,
        NiceAccess, Remapper,
    },
    FromMagiConfig,
};
#[cfg(test)]
use swc_common::chain;
use swc_common::{Mark, SyntaxContext};
use swc_ecma_ast::{
    op, AssignExpr, BinExpr, BindingIdent, CallExpr, Decl, Expr, ExprOrSpread, ExprStmt, Id, Ident,
    ModuleItem, Pat, PatOrExpr, Stmt, VarDecl, VarDeclKind, VarDeclarator,
};
#[cfg(test)]
use swc_ecma_transforms_base::{hygiene::hygiene, resolver};
use swc_ecma_transforms_testing::test;
use swc_ecma_utils::find_pat_ids;
#[cfg(test)]
use swc_ecma_visit::{as_folder, Fold};
use swc_ecma_visit::{noop_visit_mut_type, VisitMut, VisitMutWith};
pub struct IifeExpandVisitor;

impl FromMagiConfig for IifeExpandVisitor {
    fn from_config(_conf: &crate::MagiConfig) -> Self {
        Self
    }
}

// TODO: We should probably be checking for any recursiveness? I think you can still manage that even without a named function..

enum IifeExpansion {
    Expr(Expr),
    Stmts(Vec<Stmt>),
    /// The IIFE can be expanded into no value, aka undefined if it was used in an expr
    Nothing,
}

/// Attempt to evaluate a simple IIFE into an expression.
fn eval_iife(expr: &Expr) -> Option<IifeExpansion> {
    let call = expr.as_call()?;
    let callee = call.callee.as_expr()?.unwrap_parens();

    // TODO: check type parameters, for typescript code
    if call.args.is_empty() {
        return eval_no_args_iife(call, callee);
    } else if call.args.len() == 1 {
        return eval_initializer_iife(call, callee);
    } else {
        return None;
    }
}

fn eval_no_args_iife(call: &CallExpr, callee: &Expr) -> Option<IifeExpansion> {
    let fn_expr = callee.as_fn_expr()?;

    if fn_expr.ident.is_some() {
        // It is nontrivial to check if the function identifier is used anywhere, so we just
        // ignore those for now.
        return None;
    }

    let func = &fn_expr.function;

    if !func.params.is_empty() || !call.args.is_empty() {
        // The function has parameters, so it is not simple IIFE
        return None;
    }

    if func.is_async || func.is_generator {
        return None;
    }

    let body = func.body.as_ref()?;
    let stmts = &body.stmts;

    // TODO: We could at least use a constant folding pass on this, and also detect side-effect free garbage functions
    if stmts.is_empty() {
        Some(IifeExpansion::Nothing)
    } else if stmts.len() == 1 {
        // Get the return statement
        let return_stmt = stmts[0].as_return_stmt()?;

        if let Some(val) = return_stmt.arg.as_ref() {
            Some(IifeExpansion::Expr(*val.clone()))
        } else {
            Some(IifeExpansion::Nothing)
        }
    } else {
        None
    }
}

fn eval_initializer_iife(call: &CallExpr, callee: &Expr) -> Option<IifeExpansion> {
    let fn_expr = callee.as_fn_expr()?;

    if fn_expr.ident.is_some() {
        // It is nontrivial to check if the function identifier is used anywhere, so we just
        // ignore those for now.
        return None;
    }

    let func = &fn_expr.function;

    if !(func.params.len() == 1 && call.args.len() == 1) {
        // The function has zero or more than one parameters, so it is not a (simple) initializer IIFE
        return None;
    }

    let param = func.params[0].pat.as_ident()?;
    let ExprOrSpread {
        spread,
        expr: init_expr,
    } = &call.args[0];

    // TODO: I'm assuming that if spread is `Some` then it is a spread operator
    if spread.is_some() {
        // We don't support a spread operator
        return None;
    }

    // Extract initializers of the form `a = x || (x = {})` or `x || (x = {})`
    // where `assign_ident` is `a` and `init_ident` is `x`
    let (assign_ident, init_access) = extract_or_initializer_with_assign(&*init_expr)?;
    let init_access_pat_or_expr: PatOrExpr = init_access.clone().try_into().ok()?;
    let init_access_expr: Expr = init_access.clone().try_into().ok()?;

    if func.is_async || func.is_generator {
        return None;
    }

    let body = func.body.as_ref()?;

    let mut res = Vec::new();
    // We need to add the init initializer to the beginning of the statements
    // `x = x || {}`, a simplification of what was `x || (x = {})`
    res.push(Stmt::Expr(ExprStmt {
        span: call.span,
        expr: Box::new(Expr::Assign(AssignExpr {
            span: call.span,
            left: init_access_pat_or_expr,
            op: op!("="),
            // a || {}
            right: Box::new(Expr::Bin(BinExpr {
                span: call.span,
                op: op!("||"),
                left: Box::new(init_access_expr.clone()),
                right: Box::new(make_empty_object(call.span)),
            })),
        })),
    }));

    // Get the identifier that we want to use when swapping the parameter with the argument
    let use_ident = if let Some(assign_ident) = assign_ident {
        // `a = x`
        res.push(Stmt::Expr(ExprStmt {
            span: call.span,
            expr: Box::new(Expr::Assign(AssignExpr {
                span: call.span,
                left: PatOrExpr::Pat(Box::new(Pat::Ident(BindingIdent {
                    id: assign_ident.clone(),
                    type_ann: None,
                }))),
                op: op!("="),
                right: Box::new(init_access_expr.clone()),
            })),
        }));

        assign_ident
    } else {
        let new_ctxt = SyntaxContext::empty().apply_mark(Mark::fresh(Mark::root()));
        let use_ident = Ident::new("tmp".into(), call.span.with_ctxt(new_ctxt));

        // note: using `match` so that we if we extend the enum in the future then we can easily
        // notice that we need to handle the new variant
        // If it is a non-ident then we set a temporary variable equal to the value
        // Ofcourse, if it already has a tmp var (like in `a = x || (x = {})`) then we just use that
        match init_access {
            // If it is an ident then we don't even bother using the above created identifier
            NiceAccess::Ident(_) => {}
            NiceAccess::Member(_) => {
                // `let {use_ident} = x`
                // We introduce the temporary variable because we can't always directly replace
                // the `x` in `x = thing.obj;` with `thing.obj` because any intermediate
                // side-effects that might occur between the usage could lead to `thing.obj` being
                // reassigned and so `x` is no longer equal to `thing.obj` and is a different reference
                res.push(Stmt::Decl(Decl::Var(Box::new(VarDecl {
                    span: call.span,
                    kind: VarDeclKind::Let,
                    decls: vec![VarDeclarator {
                        span: call.span,
                        name: Pat::Ident(BindingIdent {
                            id: use_ident.clone(),
                            type_ann: None,
                        }),
                        init: Some(Box::new(init_access_expr)),
                        definite: false,
                    }],
                    declare: false,
                }))));
            }
        }

        use_ident
    };

    let mut remap = HashMap::new();
    let new_ctxt = SyntaxContext::empty().apply_mark(Mark::fresh(Mark::root()));

    let new_ident = Ident::new(param.sym.clone(), param.span.with_ctxt(new_ctxt));
    remap.insert(param.to_id(), new_ctxt);

    for stmt in &body.stmts {
        match stmt {
            Stmt::Decl(Decl::Var(var)) => {
                for decl in &var.decls {
                    let ids: Vec<Id> = find_pat_ids(&decl.name);
                    let ids = ids.into_iter().map(|id| {
                        (
                            id,
                            SyntaxContext::empty().apply_mark(Mark::fresh(Mark::root())),
                        )
                    });

                    remap.extend(ids);
                }
            }
            _ => {}
        }
    }

    // TODO: We should be able to just directly modify it
    let mut body = body.clone();
    body.visit_mut_with(&mut Remapper { vars: remap });

    // TODO: This expansion could cause issues if there is local variable declaration in it.
    // Currently we only allow member assignments, but you could something like
    // `x.e = function abc() {}` which if we naively expanded it into the outside scope, then it
    // have assigned a variable in the wrong scope.
    for stmt in &body.stmts {
        match stmt {
            Stmt::Expr(ExprStmt { expr, span }) => {
                let assign = get_assign_eq_expr(expr)?;

                let left = assign.left.as_expr()?.as_member()?;
                let left_ident = left.obj.as_ident()?;

                // We only support assignments to the single parameter
                if left_ident.sym != param.sym {
                    return None;
                }

                // TODO: We could support more complicated member props!
                let _prop = left.prop.as_ident()?;

                let mut rename_map = HashMap::default();

                match &init_access {
                    NiceAccess::Ident(x) => {
                        rename_map.insert(new_ident.to_id(), x.clone());
                    }
                    NiceAccess::Member(_) => {
                        rename_map.insert(new_ident.to_id(), use_ident.clone());
                    }
                }

                let mut ren = RenameIdentPass { names: rename_map };
                let mut expr = expr.clone();
                expr.visit_mut_with(&mut ren);
                res.push(Stmt::Expr(ExprStmt { expr, span: *span }));
            }
            _ => return None,
        }
    }

    Some(IifeExpansion::Stmts(res))
}

impl VisitMut for IifeExpandVisitor {
    noop_visit_mut_type!();

    fn visit_mut_stmts(&mut self, stmts: &mut Vec<Stmt>) {
        let mut new_stmts = Vec::new();
        for stmt in stmts.drain(..) {
            match stmt {
                Stmt::Expr(ExprStmt { expr, span }) => {
                    if let Some(val) = eval_iife(&expr) {
                        match val {
                            IifeExpansion::Expr(val) => {
                                new_stmts.push(Stmt::Expr(ExprStmt {
                                    expr: Box::new(val),
                                    span,
                                }));
                            }
                            IifeExpansion::Stmts(stmts) => {
                                new_stmts.extend(stmts);
                            }
                            // No need to insert undefined when it is not used by anything
                            IifeExpansion::Nothing => {}
                        }
                    } else {
                        new_stmts.push(Stmt::Expr(ExprStmt { expr, span }));
                    }
                }
                Stmt::Decl(decl) => match decl {
                    Decl::Var(mut var) => {
                        let mut decls = Vec::new();
                        for decl in var.decls.drain(..) {
                            if let Some(val) = decl.init.as_ref().and_then(|x| eval_iife(&*x)) {
                                let val = match val {
                                    IifeExpansion::Expr(val) => val,
                                    IifeExpansion::Stmts(_stmts) => {
                                        // We just don't allow this
                                        decls.push(decl);
                                        continue;
                                    }
                                    IifeExpansion::Nothing => make_undefined(decl.span),
                                };
                                decls.push(VarDeclarator {
                                    span: decl.span,
                                    name: decl.name,
                                    init: Some(Box::new(val)),
                                    definite: decl.definite,
                                });
                            } else {
                                decls.push(decl);
                            }
                        }

                        // Push the var decl with the new decls
                        new_stmts.push(Stmt::Decl(Decl::Var(Box::new(VarDecl {
                            span: var.span,
                            kind: var.kind,
                            decls,
                            declare: var.declare,
                        }))));
                    }
                    _ => new_stmts.push(Stmt::Decl(decl)),
                },
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
                ModuleItem::Stmt(stmt) => match stmt {
                    Stmt::Expr(ExprStmt { expr, span }) => {
                        if let Some(val) = eval_iife(&expr) {
                            match val {
                                IifeExpansion::Expr(val) => {
                                    new_items.push(ModuleItem::Stmt(Stmt::Expr(ExprStmt {
                                        expr: Box::new(val),
                                        span,
                                    })));
                                }
                                IifeExpansion::Stmts(stmts) => {
                                    new_items.extend(stmts.into_iter().map(ModuleItem::Stmt));
                                }
                                // No need to insert undefined when it is not used by anything
                                IifeExpansion::Nothing => {}
                            }
                        } else {
                            new_items.push(ModuleItem::Stmt(Stmt::Expr(ExprStmt { expr, span })));
                        }
                    }
                    Stmt::Decl(decl) => match decl {
                        Decl::Var(mut var) => {
                            let mut decls = Vec::new();
                            for decl in var.decls.drain(..) {
                                if let Some(val) = decl.init.as_ref().and_then(|x| eval_iife(&*x)) {
                                    let val = match val {
                                        IifeExpansion::Expr(val) => val,
                                        IifeExpansion::Stmts(_stmts) => {
                                            // We just don't allow this
                                            decls.push(decl);
                                            continue;
                                        }
                                        IifeExpansion::Nothing => make_undefined(decl.span),
                                    };
                                    decls.push(VarDeclarator {
                                        span: decl.span,
                                        name: decl.name,
                                        init: Some(Box::new(val)),
                                        definite: decl.definite,
                                    });
                                } else {
                                    decls.push(decl);
                                }
                            }

                            // Push the var decl with the new decls
                            new_items.push(ModuleItem::Stmt(Stmt::Decl(Decl::Var(Box::new(
                                VarDecl {
                                    span: var.span,
                                    kind: var.kind,
                                    decls,
                                    declare: var.declare,
                                },
                            )))));
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

#[cfg(test)]
fn tr() -> impl Fold {
    use swc_ecma_transforms_base::fixer::fixer;

    let unresolved_mark = Mark::new();
    let top_level_mark = Mark::new();

    chain!(
        // We have to run the resolver if we want to have the correct scope information for renaming properly
        resolver(unresolved_mark, top_level_mark, false),
        as_folder(IifeExpandVisitor),
        hygiene(),
        fixer(None),
    )
}

test!(
    Default::default(),
    |_| as_folder(IifeExpandVisitor),
    iife_expand1_sanity1,
    // TODO: We can actually do better than this, since this is side-effect free and returns nothing
    "var a = 1; (function() { var b = 2; })();" // "var a = 1; (function() { var b = 2; })();"
);
test!(
    Default::default(),
    |_| as_folder(IifeExpandVisitor),
    iife_expand1_sanity3,
    // TODO: We can do better than this. Especially since SWC keeps the variables with separate identifiers for their scopes, so I think it can just automatically deduplicate the names?
    "var a = 1, b = 3; (function() { var b = 2; })();" // "var a = 1, b = 3; (function() { var b = 2; })();"
);
test!(
    Default::default(),
    |_| as_folder(IifeExpandVisitor),
    iife_expand1_sanity2,
    // TODO: We can actually just expand this out
    "var a = 1; (function() { console.log('blah') })();" // "var a = 1; (function() { console.log('blah') })();"
);
test!(
    Default::default(),
    |_| as_folder(IifeExpandVisitor),
    iife_expand2,
    "var a = 1; (function() { })();" // "var a = 1;"
);

test!(
    Default::default(),
    |_| as_folder(IifeExpandVisitor),
    iife_expand3,
    "var a = (function() { return 2; })();" // "var a = 2;"
);

test!(
    Default::default(),
    |_| as_folder(IifeExpandVisitor),
    iife_expand5,
    "var a = (function() { })();" // "var a = undefined;"
);

test!(
    Default::default(),
    |_| tr(),
    iife_expand6,
    "var a; (function(e) { e.j = 5; })(a || (a = {}));" // "var a; a = a || {}; a.j = 5;"
);

test!(
    Default::default(),
    |_| tr(),
    iife_expand4,
    "var a,x; (function(e) { e.j = 5 })(a = x || (x = {}));" // We expand this as `x.j` because we can then easily apply a variable removal pass
                                                             // "var a,x; x = x || {}; a = x; x.j = 5;"
);

test!(
    Default::default(),
    |_| tr(),
    iife_expand7,
    "var a,x; (function(e) { e.j = function (e) { e.k = 5 } })(a = x || (x = {}));" // We expand this as `x.j` because we can then easily apply a variable removal pass
                                                                                    // "var a,x; x = x || {}; a = x; x.j = function (e) { e.k = 5 };"
);

test!(
    Default::default(),
    |_| tr(),
    iife_expand_sanity7,
    // we shouldn't replace the variable name naively
    "var a,x; (function(e) { e.j = function (x) { e.k = 5 } })(a = x || (x = {}));" // "var a,x; x = x || {}; a = x; x.j = function (x1) { x.k = 5 };"
);

test!(
    Default::default(),
    |_| tr(),
    iife_expand11,
    "var p; (function (e1) { e1.a = \"a\"; })(p = exports.Thing || (exports.Thing = {}));" // TODO: We should be able to remove the `p` here
                                                                                           // "var p; exports.Thing = exports.Thing || {}; p = exports.Thing; p.a = \"a\";"
);
test!(
    Default::default(),
    |_| tr(),
    iife_expand12,
    "var p; (function (e1) { e1.a = function () { return p.b; }; })(p = exports.Thing || (exports.Thing = {}));"
    // This is a case where we can't just remove the `p` because it is used in the function
    // and, unless we do more complicated analyses, we don't know that `exports.Thing` isn't 
    // replaced by a different object, thus leaving `p` as a lone reference to the original object
    // "var p; exports.Thing = exports.Thing || {}; p = exports.Thing; p.a = function () { return p.b; };"
);

test!(
    Default::default(),
    |_| tr(),
    iife_expand14,
    "(function (e1) { e1.a = 'a'; })(exports.Thing || (exports.Thing = {}));" // TODO: We could remove the `tmp` here
                                                                              // "exports.Thing = exports.Thing || {}; let tmp = exports.Thing; tmp.a = 'a'"
);
test!(
    Default::default(),
    |_| tr(),
    iife_expand13,
    "(function (e1) { e1.a = function () { return e1.b; }; })(exports.Thing || (exports.Thing = {}));"
    // "exports.Thing = exports.Thing || {}; let tmp = exports.Thing; tmp.a = function () { return tmp.b; };"
);

test!(
    Default::default(),
    |_| tr(),
    iife_expand8,
    "var a,x; (function(e) { e.j = function () { e.k = 5 } })(a = x || (x = {}));" // We expand this as `x.j` because we can then easily apply a variable removal pass
                                                                                   // "var a,x; x = x || {}; a = x; x.j = function () { x.k = 5 };"
);

test!(
    Default::default(),
    |_| tr(),
    iife_expand9,
    "var d; (function(e1) { e1.is = function(e1) { return o.func(e1); }; })(d || (d = {}));" // "var d; d = d || {}; d.is = function(e1) { return o.func(e1); };"
);

test!(
    Default::default(),
    |_| tr(),
    iife_expand10,
    "(function(e1) { e1.type = new i.thing(\"aaa\");})(l || (l = {}));" // "l = l || {}; l.type = new i.thing(\"aaa\");"
);

// test!(
//     Default::default(),
//     |_| as_folder(IifeExpandVisitor),
//     iife_expand9,
//     "var d; (function(e1) { e1.is = function(e1) { return o.func(e1); }; })(d || (d = {}));",
//     // We expand this as `x.j` because we can then easily apply a variable removal pass
//     "var d; d = d || {}; d.is = function(e1) { return o.func(e1); };"
// );

// TODO: It would be nice to delete this, but it has the issue that it is not *necessarily* side-effect free due to getters. Also, it'd be better to do some constant propagation pass rather than specially handling this case, because it isn't common.
// test!(
//     Default::default(),
//     |_| as_folder(IifeExpandVisitor),
//     iife_expand7,
//     "var a; (function(e) { e.j })(a = x || (x = {}));",
//     "var a; a = x || (x = {});"
// );

// TODO: I feel like there's probably edge cases where this doesn't work right!
