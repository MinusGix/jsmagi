use std::collections::HashMap;

use swc_ecma_ast::{Id, Ident, Module, Script};

use swc_ecma_visit::{noop_visit_mut_type, VisitMut, VisitMutWith};

use crate::eval::contains_eval;

/// Visitation pass for renaming identifiers to other identifiers  
/// This is meant to allow for hygienic renaming of identifiers, because it allows
/// keeping ctxt information so that the identifiers are still considered the same
/// and so then a hygiene pass can ensure the variable names are correct.
pub struct RenameIdentPass {
    pub names: HashMap<Id, Ident>,
}
impl VisitMut for RenameIdentPass {
    noop_visit_mut_type!();

    fn visit_mut_ident(&mut self, i: &mut Ident) {
        if let Some(new_name) = self.names.get(&i.to_id()) {
            i.sym = new_name.sym.clone();
            i.span.ctxt = new_name.span.ctxt;
        }
    }

    fn visit_mut_module(&mut self, m: &mut Module) {
        if contains_eval(m, true) {
            m.visit_mut_children_with(self);
        } else {
            todo!()
        }
    }

    fn visit_mut_script(&mut self, s: &mut Script) {
        if contains_eval(s, true) {
            s.visit_mut_children_with(self);
        } else {
            todo!()
        }
    }
}
