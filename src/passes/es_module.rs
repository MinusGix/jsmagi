use std::collections::HashMap;

use swc_atoms::{js_word, JsWord};

use swc_ecma_ast::{Decl, Expr, ModuleItem, Pat, Prop, PropOrSpread};
use swc_ecma_transforms_base::rename::rename;
use swc_ecma_transforms_testing::test;
use swc_ecma_utils::ident::IdentLike;
#[cfg(test)]
use swc_ecma_visit::as_folder;
use swc_ecma_visit::{noop_visit_mut_type, VisitMut, VisitMutWith};

use crate::{FromMagiConfig, MagiConfig};

// TODO: analyze what the module sets on `exports.*` and collect those into a typescript interface
// and maybe a comment
// We can then have the type information refined as needed. We can also make it clear our uncertainty about the types by making a default '[key: string]: any' interface prop

/// Looks for an object at the root with keys that are functions of the form `(e, t, n) => { ... }`
/// and renames them to `module, exports, require` if they are found.
pub struct EsModuleRenameVisitor {
    typescript: bool,
}
impl FromMagiConfig for EsModuleRenameVisitor {
    fn from_config(conf: &MagiConfig) -> Self {
        Self {
            typescript: conf.typescript,
        }
    }
}

fn visit_mut_module_items(_typescript: bool, n: &mut Vec<ModuleItem>) -> Option<()> {
    // TODO: this might benefit from being more general?

    // If there is an iife at the root, go into it
    let stmt = n.get_mut(0)?.as_mut_stmt()?;
    let expr = stmt.as_mut_expr()?.expr.as_mut();
    let call = expr.as_mut_call()?;

    let callee = call.callee.as_mut_expr()?.unwrap_parens_mut();

    let block = match callee {
        Expr::Arrow(arrow) => {
            // TODO: technically this ignores single expression arrow functions
            let block = arrow.body.as_mut_block_stmt()?;
            block
        }
        Expr::Fn(func) => {
            let body = func.function.body.as_mut()?;
            body
        }
        _ => return None,
    };

    let decl = block.stmts.get_mut(0)?.as_mut_decl()?;
    let Decl::Var(decl) = decl else { return None; };

    // We assume that it is the first variable
    let decl = decl.decls.get_mut(0)?;

    // Get the value it is being initialized to
    let init = decl.init.as_deref_mut()?.as_mut_object()?;

    for prop in &mut init.props {
        // TODO: is it actually okay to skip over these?
        let PropOrSpread::Prop(prop) = prop else { continue; };
        let Prop::KeyValue(key_value) = prop.as_ref() else { continue; };

        let params = match key_value.value.as_ref() {
            Expr::Arrow(arrow) => {
                let params = &arrow.params;
                if params.len() != 3 {
                    continue;
                }

                (&params[0], &params[1], &params[2])
            }
            Expr::Fn(func) => {
                let params = &func.function.params;
                if params.len() != 3 {
                    continue;
                }

                (&params[0].pat, &params[1].pat, &params[2].pat)
            }
            _ => continue,
        };

        let (Pat::Ident(p1), Pat::Ident(p2), Pat::Ident(p3)) = params else { continue; };

        if !idents_match_req(p1, p2, p3) {
            continue;
        }

        let mut renames = HashMap::default();
        renames.insert(p1.id.to_id(), js_word!("module"));
        renames.insert(p2.id.to_id(), JsWord::from("exports"));
        renames.insert(p3.id.to_id(), js_word!("require"));

        let mut renamer = rename(&renames);
        prop.visit_mut_children_with(&mut renamer);
    }

    Some(())
}

impl VisitMut for EsModuleRenameVisitor {
    noop_visit_mut_type!();

    fn visit_mut_module_items(&mut self, n: &mut Vec<ModuleItem>) {
        visit_mut_module_items(self.typescript, n);

        n.visit_mut_children_with(self);
    }
}

/// Check if the given idents match `e`, `t`, `n`
fn idents_match_req<T: IdentLike>(first: &T, second: &T, third: &T) -> bool {
    first.to_id().0 == JsWord::from("e")
        && second.to_id().0 == JsWord::from("t")
        && third.to_id().0 == JsWord::from("n")
}

test!(
    Default::default(),
    |_| as_folder(EsModuleRenameVisitor { typescript: false }),
    rename1,
    "(() => { var e1 = { 428: (e, t, n) => { t.thing = 5; let j = n(524); } }; })();",
    "(() => { var e1 = { 428: (module, exports, require) => { exports.thing = 5; let j = require(524); } }; })();"
);
