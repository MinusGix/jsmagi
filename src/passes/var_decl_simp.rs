//! Simplify variable declarations that have an initializer separate from them.
//! Example:
//! ```js
//! var l;
//! var j;
//! var k;
//! l = 0;
//! ```
//! becomes
//! ```js
//! var l = 0;
//! var j;
//! var k;
//! ```
//! ----
//!
//! ```js
//! var l;
//! var j;
//! var k;
//! l = l || {};
//! ```
//! becomes
//! ```js
//! var l = {};
//! var j;
//! var k;
//! ```
//! because `l` would always be undefined, thus resulting in an empty object.  
//!
//! This pass cannot transform all variable declarations

use std::collections::{HashMap, HashSet};

use swc_atoms::JsWord;
use swc_common::{collections::AHashSet, util::take::Take};
use swc_ecma_ast::{
    BindingIdent, Decl, Expr, ExprOrSpread, ModuleItem, Stmt, VarDecl, VarDeclKind,
};
use swc_ecma_transforms_testing::test;
use swc_ecma_utils::collect_decls;
use swc_ecma_visit::{as_folder, VisitMut, VisitMutWith};

use crate::{util::StmtsMut, FromMagiConfig};

pub struct VarDeclSimp;
impl FromMagiConfig for VarDeclSimp {
    fn from_config(_conf: &crate::MagiConfig) -> Self {
        Self
    }
}

impl VisitMut for VarDeclSimp {
    fn visit_mut_stmts(&mut self, stmts: &mut Vec<Stmt>) {
        var_decl_simp(stmts.into());

        stmts.visit_mut_children_with(self);
    }

    fn visit_mut_module_items(&mut self, items: &mut Vec<ModuleItem>) {
        var_decl_simp(items.into());

        items.visit_mut_children_with(self);
    }

    // fn visit_mut_module_items(&mut self, items: &mut Vec<ModuleItem>) {
    //     for item in items {
    //         if let ModuleItem::Stmt(stmt) = item {
    //             visit_mut_stmt(stmt);
    //         }
    //     }

    //     items.visit_mut_children_with(self);
    // }
}

enum Edit {
    /// (tracked_variables_idx, new_init)
    ChangeInit(usize, Box<Expr>),
    /// (stmts_idx)
    RemoveStmt(usize),
    None,
}

struct TrackedVariable {
    stmts_idx: usize,
    decl_idx: usize,
    ident: JsWord,
    init: Option<Box<Expr>>,
}
type TrackedVariables = Vec<TrackedVariable>;

fn find_tv_idx(
    tracked_variables: &TrackedVariables,
    stmts_idx: usize,
    decl_idx: usize,
) -> Option<&TrackedVariable> {
    // use iterator adaptors
    tracked_variables
        .iter()
        .rev()
        .find(|tv| tv.stmts_idx == stmts_idx && tv.decl_idx == decl_idx)
}

fn var_decl_simp(mut stmts: StmtsMut<'_>) -> Option<()> {
    let mut tracked_variables = TrackedVariables::new();

    let mut edits: Vec<Edit> = Vec::new();

    // let mut iter = stmts.iter_idx();
    for (i, stmt) in stmts.iter_idx() {
        match stmt {
            Stmt::Decl(x) => handle_decl(&mut tracked_variables, i, x)?,
            Stmt::Expr(x) => handle_expr(&tracked_variables, &mut edits, i, &x.expr)?,
            _ => {
                println!("stmt: {:?}", stmt);
                return None;
            }
        }

        // for now this just simplifies variables that aren't ever set in their initializer
    }

    // for edit in edits {
    //     match edit {
    //         Edit::ChangeInit(idx, init) => {
    //             let decl = stmts.get_mut
    //         },
    //         Edit::RemoveStmt(_) => todo!(),
    //     }
    // }
    for (stmts_idx, stmt) in stmts.iter_mut_idx() {
        for edit in edits.iter_mut() {
            match edit {
                Edit::ChangeInit(idx, init) => {
                    let var = &tracked_variables[*idx];
                    if stmts_idx == var.stmts_idx {
                        let decl = stmt.as_mut_decl().unwrap();
                        let var_decl = decl.as_mut_var().unwrap();
                        let decl = &mut var_decl.decls[var.decl_idx];
                        assert!(decl.init.is_none(), "We don't currently support more complicated initializer changing in this pass so this is a bug");
                        decl.init = Some(init.clone());

                        *edit = Edit::None;
                    }
                }
                Edit::RemoveStmt(idx) => {
                    if stmts_idx == *idx {
                        stmt.take();

                        *edit = Edit::None;
                    }
                }
                Edit::None => {}
            }
        }
    }

    Some(())
}

fn handle_decl(
    tracked_variables: &mut TrackedVariables,
    stmts_idx: usize,
    decl: &Decl,
) -> Option<()> {
    // TODO: we could allow more variable declaration types
    let var_decl = decl.as_var()?;
    for (decl_idx, decl) in var_decl.decls.iter().enumerate() {
        // TODO: just use ?
        if let Some(ident) = decl.name.as_ident() {
            tracked_variables.push(TrackedVariable {
                stmts_idx,
                decl_idx,
                ident: ident.sym.clone(),
                init: decl.init.clone(),
            });
        } else {
            println!("Found non ident");
            return None;
        }
    }

    Some(())
}

fn handle_expr(
    tracked_variables: &TrackedVariables,
    edits: &mut Vec<Edit>,
    i: usize,
    expr: &Expr,
) -> Option<()> {
    /*match expr {
        Expr::Call(call) => {
            // If it is Object.defineProperty on `exports` then we ignore it
            let callee = call.callee.as_expr()?;
            let callee = callee.as_member()?;

            if let Some((obj, prop)) = callee.obj.as_ident().zip(callee.prop.as_ident()) {
                if obj.sym == *"Object" && prop.sym == *"defineProperty" {
                    let arg = &call.args[0];
                    let arg = arg.expr.as_ident()?;
                    if arg.sym != *"exports" {
                        return None;
                    }
                } else {
                    println!("Found non Object.defineProperty");
                    return None;
                }
            } else {
                println!("Found non Object.defineProperty (sub)");
                return None;
            }
        }
    }

    todo!()*/
    None
}

test!(
    Default::default(),
    |_| as_folder(VarDeclSimp),
    single_variable,
    "let n;",
    "let n;"
);

test!(
    Default::default(),
    |_| as_folder(VarDeclSimp),
    single_variable_with_init,
    "let n = 0;",
    "let n = 0;"
);

test!(
    Default::default(),
    |_| as_folder(VarDeclSimp),
    single_variable_def,
    "let n; n = 0;",
    "let n = 0;"
);

test!(
    Default::default(),
    |_| as_folder(VarDeclSimp),
    single_variable_def_with_init,
    "let n; n = n || {};",
    "let n = {};"
);

test!(
    Default::default(),
    |_| as_folder(VarDeclSimp),
    multiple_variables_def_with_init,
    "let n; let c; n = n || {}; c = c || {};",
    "let n = {}; let c = {};"
);
