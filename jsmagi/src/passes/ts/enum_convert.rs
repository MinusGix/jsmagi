use swc_atoms::JsWord;
use swc_common::{Mark, SyntaxContext};
use swc_ecma_ast::{
    op, AssignExpr, BinExpr, BindingIdent, CallExpr, Callee, Expr, ExprOrSpread, ExprStmt, FnExpr,
    Ident, Lit, ModuleItem, Pat, PatOrExpr, Stmt, TsEnumDecl, TsEnumMember, TsEnumMemberId,
};

use swc_ecma_transforms_testing::test;

use swc_ecma_utils::member_expr;
use swc_ecma_visit::{noop_visit_mut_type, VisitMut, VisitMutWith};

use crate::{
    util::{extract_or_initializer_with_assign, get_assign_eq_expr, make_empty_object, NiceAccess},
    FromMagiConfig, MagiConfig, RandomName,
};

/// This converts IIFE constructed enums in Javascript to their Typescript equivalent.
pub struct EnumConvert {
    random_name: RandomName,
}
impl FromMagiConfig for EnumConvert {
    fn from_config(conf: &MagiConfig) -> Self {
        Self {
            random_name: conf.random_name(),
        }
    }
}

/// Returns `Some(function being called, arguments to the function)`
fn get_iife(expr: &Expr) -> Option<(&FnExpr, &Vec<ExprOrSpread>)> {
    let call = expr.as_call()?;
    let callee = call.callee.as_expr()?.unwrap_parens();

    let fn_expr = callee.as_fn_expr()?;

    Some((fn_expr, &call.args))
}

// The javascript output of typescript enums are of the form:
// ```js
// (function (e) {
//     e[e["A"] = 0] = "A";
//     e[e["B"] = 1] = "B";
//     e[e["C"] = 2] = "C";
// })(exports.MyEnum || (exports.MyEnum = {}));
// ```
// Sometimes the argument is of the form:
/// ```js
/// })(p = exports.MyEnum || (exports.MyEnum = {}));
/// ```
fn visit_stmt(random_name: &RandomName, stmt: &Stmt) -> Option<Vec<Stmt>> {
    let ExprStmt { expr, span } = stmt.as_expr()?;

    let span = span.clone();

    let (fn_expr, args) = get_iife(expr)?;
    let func = &fn_expr.function;

    // We only support one parameter because that's how enums are written
    if args.len() != 1 {
        return None;
    }

    let param = func.params[0].pat.as_ident()?;
    let ExprOrSpread { spread, expr: arg } = &args[0];

    // Not supported.
    if spread.is_some() {
        return None;
    }

    let (assign_ident, init_access) = extract_or_initializer_with_assign(&*arg)?;
    let init_access_pat_or_expr: PatOrExpr = init_access.clone().try_into().ok()?;
    let init_access_expr: Expr = init_access.clone().try_into().ok()?;

    // These are almost certainly not enums.
    if func.is_async || func.is_generator {
        return None;
    }

    let body = func.body.as_ref()?;

    let mut res = Vec::new();

    // Note: assumes no side effects from assignment
    // We need to add the init initializer to the beginning of the statements
    // `x = x || {}`, a simplification of what was `x || (x = {})`
    res.push(Stmt::Expr(ExprStmt {
        span,
        expr: Box::new(Expr::Assign(AssignExpr {
            span,
            left: init_access_pat_or_expr.clone(),
            op: op!("="),
            // a || {}
            right: Box::new(Expr::Bin(BinExpr {
                span,
                op: op!("||"),
                left: Box::new(init_access_expr.clone()),
                right: Box::new(make_empty_object(span)),
            })),
        })),
    }));

    let use_v: Expr = if let Some(assign_ident) = assign_ident {
        // `a = x`
        res.push(Stmt::Expr(ExprStmt {
            span: span,
            expr: Box::new(Expr::Assign(AssignExpr {
                span: span,
                left: PatOrExpr::Pat(Box::new(Pat::Ident(BindingIdent {
                    id: assign_ident.clone(),
                    type_ann: None,
                }))),
                op: op!("="),
                right: Box::new(init_access_expr.clone()),
            })),
        }));

        assign_ident.into()
    } else {
        init_access_expr.clone()
    };

    let enum_id = {
        let id = init_access_expr
            .as_member()
            .and_then(|mem| {
                mem.obj
                    .as_ident()
                    .filter(|id| id.sym == JsWord::from("exports"))
                    .and_then(|_| mem.prop.as_ident().cloned())
                    .map(|id| id.sym)
            })
            .unwrap_or_else(|| JsWord::from(random_name.get("en")));
        let new_ctxt = SyntaxContext::empty().apply_mark(Mark::fresh(Mark::root()));
        Ident::new(id, span.with_ctxt(new_ctxt))
    };

    let mut enum_res = TsEnumDecl {
        // TODO: better span? Maybe just the iife.
        span,
        // We assume that we shouldn't mark the enum as declared elsewhere.
        // Not entirely sure that this is always correct, but will typescript behave badly if we
        // assume this?
        declare: false,
        // TODO(minor): Allow the user to force generation of constant enums.
        // It isn't constant because we are inferring it from a non constant enum declaration!
        is_const: false,
        id: enum_id.clone(),
        members: Vec::new(),
    };

    for stmt in &body.stmts {
        let ExprStmt { expr, span } = stmt.as_expr()?;
        let assign = get_assign_eq_expr(expr)?;

        let left = assign.left.as_expr()?;

        let left = left.as_member()?;
        let left_ident = left.obj.as_ident()?;

        // We only support assignments to the single parameter
        if left_ident.sym != param.sym {
            return None;
        }
        // We only handle props of the form `a[a.B = 1] = "B"`

        // TODO: Check if the string literal is the same as the field name
        // "B"
        let Lit::Str(right_str) = assign.right.as_lit()? else {
            return None;
        };

        // `a[a.B = 1]`
        let prop = left.prop.as_computed()?;

        // `a.B = 1`
        let assign = prop.expr.unwrap_parens().as_assign()?;

        // `a.B`
        let member = assign.left.as_expr()?.as_member()?;

        // `a`
        let member_ident = member.obj.as_ident()?;
        // We ensure that `member_ident` is the same as `left_ident`
        if member_ident.sym != left_ident.sym {
            return None;
        }

        // `B`
        let prop_name = member.prop.as_ident()?;
        // We ensure that `prop_name` has the same name as the string we are assigning to
        if prop_name.sym != right_str.value {
            return None;
        }

        // `1`
        let init = assign.right.as_lit()?;
        // TODO: Are there other enum types we might detect here?
        let Lit::Num(init) = init else {
            return None;
        };

        // TODO: Check whether numbers have repeats?

        let member = TsEnumMember {
            span: span.clone(),
            id: TsEnumMemberId::Ident(prop_name.clone()),
            init: Some(Box::new(init.clone().into())),
        };

        enum_res.members.push(member);

        // let mut rename_map = HashMap::default();

        // match &init_access {
        //     NiceAccess::Ident(x) => {
        //         rename_map.insert(new_ident.to_id(), x.clone());
        //     }
        //     NiceAccess::Member(_) => {
        //         rename_map.insert(new_ident.to_id(), use_ident.clone());
        //     }
        // }

        // let mut ren = RenameIdentPass { names: rename_map };
        // let mut expr = expr.clone();
        // expr.visit_mut_with(&mut ren);
        // res.push(Stmt::Expr(ExprStmt { expr, span: *span }));
    }

    res.push(enum_res.into());

    // Then we want to do `Object.assign(exports.Thing, Thing);` where `Thing` is the enum id
    // *if* it is of the form `(p = exports.Thing || (exports.Thing = {}))`
    // or, if we just have `(p || (p = {}))` then `Object.assign(p, enum_$42)`.

    {
        let field_access = member_expr!(span, Object.assign);

        let target: Expr = match init_access {
            NiceAccess::Ident(_) => use_v.clone().into(),
            NiceAccess::Member(x) => x.into(),
        };

        let call = CallExpr {
            span,
            callee: Callee::Expr(Box::new(field_access.into())),
            args: vec![target.into(), Expr::from(enum_id).into()],
            type_args: None,
        };

        res.push(Stmt::Expr(ExprStmt {
            expr: call.into(),
            span,
        }));
    }

    Some(res)
}

impl VisitMut for EnumConvert {
    noop_visit_mut_type!();

    fn visit_mut_stmts(&mut self, stmts: &mut Vec<Stmt>) {
        let mut new_stmts = Vec::new();
        for stmt in stmts.drain(..) {
            if let Some(stmts) = visit_stmt(&self.random_name, &stmt) {
                new_stmts.extend(stmts);
            } else {
                new_stmts.push(stmt);
            }
        }

        *stmts = new_stmts;

        stmts.visit_mut_children_with(self);
    }

    fn visit_mut_module_items(&mut self, items: &mut Vec<ModuleItem>) {
        let mut new_items = Vec::new();
        for item in items.drain(..) {
            if let ModuleItem::Stmt(stmt) = &item {
                if let Some(stmts) = visit_stmt(&self.random_name, &stmt) {
                    new_items.extend(stmts.into_iter().map(ModuleItem::Stmt));
                    continue;
                }
            }

            new_items.push(item);
        }

        *items = new_items;

        items.visit_mut_children_with(self);
    }
}

#[cfg(test)]
const TS_SYN: swc_ecma_parser::Syntax =
    swc_ecma_parser::Syntax::Typescript(swc_ecma_parser::TsConfig {
        tsx: false,
        decorators: false,
        dts: false,
        no_early_errors: true,
        disallow_ambiguous_jsx_like: false,
    });

#[cfg(test)]
fn enum_convert(
    _: &mut swc_ecma_transforms_testing::Tester<'_>,
) -> swc_ecma_visit::Folder<EnumConvert> {
    swc_ecma_visit::as_folder(EnumConvert {
        random_name: RandomName::default(),
    })
}

test!(
    TS_SYN,
    enum_convert,
    enum_convert1,
    "(function(e1) { e1[e1.A = 0] = \"A\"; e1[e1.B = 1] = \"B\"; e1[e1.C = 2] = \"C\"; })(p = exports.Thing || (exports.Thing = {}));"
    // TODO: actually, naming the enum like this has the issue that there could be a variable named `Thing`. Typically not, because minifiers, but there could be. Though, maybe swc's unique naming would prevent that?
    // This is more verbose than what the original probably was, but we want to match the behavior
    // without unecessarily assuming that it is sane.
    // Hopefully a different pass can remove the unecessary verbosity.
    // "exports.Thing = exports.Thing || {}; p = exports.Thing; enum Thing { A = 0, B = 1, C = 2 }\n Object.assign(exports.Thing, Thing);"
);

test!(
    TS_SYN,
    enum_convert,
    enum_convert2,
    "(function(e1) { e1[e1.A = 0] = \"A\"; e1[e1.B = 1] = \"B\"; e1[e1.C = 2] = \"C\"; })(exports.Thing || (exports.Thing = {}));"
    // "exports.Thing = exports.Thing || {}; enum Thing { A = 0, B = 1, C = 2 }\n Object.assign(exports.Thing, Thing);"
);

test!(
    TS_SYN,
    enum_convert,
    enum_convert3,
    "(function(e1) { e1[e1.A = 0] = \"A\"; })(p = exports.Thing || (exports.Thing = {}));" // "exports.Thing = exports.Thing || {}; p = exports.Thing; enum Thing { A = 0 }\n Object.assign(exports.Thing, Thing);"
);

test!(
    TS_SYN,
    enum_convert,
    enum_convert4,
    // This version doesn't have a `p = exports.Thing` statement.
    // But we can't assume here, despite it *probably* being okay, that `w` is undefined/empty-object.
    // So we have to be more verbose and declare an anonymous enum. Unfortunately, TS doesn't have
    // anonymous enums, so we have to give it a garbage name.
    "(function (e1) { e1[e1.A = 1] = \"A\"; e1[e1.B = 2] = \"B\"; })(w || (w = {}));" // TODO: it might be nice to have the enum name be related to the variable name, like w_$000
                                                                                      // "w = w || {}; enum en_$0000 { A = 1, B = 2 }\n Object.assign(w, en_$0000);"
);

test!(
    TS_SYN,
    enum_convert,
    non_enum_convert1,
    // Should not convert this to an enum. Though if anything actually outputs this, it might be desirable.
    "(function (e) { e[0] = \"A\"; e[1] = \"B\"; })(w || (w = {}));" // "(function (e) { e[0] = \"A\"; e[1] = \"B\"; })(w || (w = {}));"
);
