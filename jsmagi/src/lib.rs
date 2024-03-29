use std::{cell::Cell, path::Path, rc::Rc, sync::Arc};

use swc::{
    config::{Config, Options, SourceMapsConfig},
    BoolConfig, Compiler, TransformOutput,
};
use swc_common::{
    chain,
    errors::{ColorConfig, Handler},
    FileName, SourceMap,
};
use swc_ecma_ast::EsVersion;
use swc_ecma_parser::{EsConfig, Syntax};
use swc_ecma_transforms_base::pass::noop;
use swc_ecma_visit::{as_folder, Fold};

use crate::passes::{
    es_module::EsModuleRenameVisitor, iife_expand::IifeExpandVisitor,
    init_assignment::InitAssignmentVisitor, nested_assignment::NestedAssignmentVisitor,
    not_iife::NotIifeVisitor, not_lit::NotLitVisitor, seq_expand::SeqExpandVisitor,
    ts::enum_convert::EnumConvert, var_decl_expand::VarDeclExpand,
    void_to_undefined::VoidToUndefinedVisitor,
};

pub mod eval;

pub mod passes;
pub mod rename;

pub mod effect;
pub mod util;

#[derive(Debug, Clone)]
pub struct MagiConfig {
    /// Whether it should generate typescript code.  
    pub typescript: bool,
    /// Whether it should assume that the file is compiled as ES Modules.
    pub assume_es_modules: bool,
    // TODO: Option to be more careful about eval
    // TODO: Option to be more careful about property accessing, potentially due to getters/setters/proxies.
    //   Though, it would be good to allow the user to specify a whitelist/blacklist of functions
    //   that they believe are likely 'safe'
    pub random_name: RandomName,
}
impl MagiConfig {
    pub(crate) fn get_passes(&self) -> impl Fold {
        as_folder(chain!(
            // resolver(unresolved_mark, top_level_mark, false),
            SeqExpandVisitor::from_config(self),
            VoidToUndefinedVisitor::from_config(self),
            NotLitVisitor::from_config(self),
            NotIifeVisitor::from_config(self),
            InitAssignmentVisitor::from_config(self),
            NestedAssignmentVisitor::from_config(self),
            VarDeclExpand::from_config(self),
            IifeExpandVisitor::from_config(self),
            // TODO: make toggleable
            EsModuleRenameVisitor::from_config(self),
            EnumConvert::from_config(self),
        ))
    }

    pub fn random_name(&self) -> RandomName {
        self.random_name.clone()
    }
}

#[derive(Debug, Clone)]
pub struct RandomName {
    id: Rc<Cell<usize>>,
}
// Mostly for testing
impl Default for RandomName {
    fn default() -> Self {
        Self {
            id: Rc::new(Cell::new(0)),
        }
    }
}
impl RandomName {
    pub fn new(id: Rc<Cell<usize>>) -> Self {
        Self { id }
    }

    pub fn get(&self, prefix: &str) -> String {
        let id = self.id.get();
        self.id.set(id + 1);
        format!("{}_${:04}", prefix, id)
    }
}

pub trait FromMagiConfig {
    fn from_config(conf: &MagiConfig) -> Self;
}

pub fn transform(filename: impl AsRef<Path>, conf: MagiConfig) -> String {
    let filename = filename.as_ref();
    let filename_text = filename.to_string_lossy().into_owned();
    let code = std::fs::read_to_string(filename).unwrap();

    let passes = conf.get_passes();

    let source_map: Arc<SourceMap> = Default::default();
    let source_file =
        source_map.new_source_file(FileName::Custom(filename_text.clone()), code.to_string());
    let handler =
        Handler::with_tty_emitter(ColorConfig::Auto, true, false, Some(source_map.clone()));

    let compiler = Compiler::new(source_map);

    let transformed = compiler.process_js_with_custom_pass(
        source_file,
        None,
        &handler,
        &Options {
            config: Config {
                jsc: swc::config::JscConfig {
                    target: Some(EsVersion::Es2022),
                    syntax: Some(Syntax::Es(EsConfig {
                        jsx: true,
                        ..Default::default()
                    })),
                    loose: BoolConfig::new(Some(false)),
                    external_helpers: BoolConfig::new(Some(false)),
                    keep_class_names: BoolConfig::new(Some(false)),
                    ..Default::default()
                },
                ..Default::default()
            },
            source_file_name: Some(filename_text.clone()),
            source_maps: Some(SourceMapsConfig::Bool(true)),
            ..Default::default()
        },
        Default::default(),
        |_| passes,
        |_| noop(),
    );

    handler.abort_if_errors();

    let TransformOutput { code, map: _ } = transformed.unwrap();

    code
}
