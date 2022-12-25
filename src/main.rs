use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use init_assignment::InitAssignmentVisitor;

use swc::{
    config::{Config, Options, SourceMapsConfig},
    BoolConfig, Compiler, TransformOutput,
};
use swc_common::{
    chain,
    errors::{ColorConfig, Handler},
    FileName, Globals, SourceMap, GLOBALS,
};
use swc_ecma_ast::EsVersion;
use swc_ecma_parser::{EsConfig, Syntax};
use swc_ecma_transforms_base::pass::noop;
use swc_ecma_visit::as_folder;
use void_to_undefined::VoidToUndefinedVisitor;

use crate::{
    es_module::EsModuleRenameVisitor, iife_expand::IifeExpandVisitor,
    nested_assignment::NestedAssignmentVisitor, not_iife::NotIifeVisitor, not_lit::NotLitVisitor,
    seq_expand::SeqExpandVisitor, var_decl_expand::VarDeclExpand,
};

pub mod es_module;
pub mod eval;
pub mod iife_expand;
pub mod init_assignment;
pub mod nested_assignment;
pub mod not_iife;
pub mod not_lit;
pub mod rename;
pub mod seq_expand;
pub mod util;
pub mod var_decl_expand;
pub mod void_to_undefined;

fn main() {
    let path = PathBuf::from("example/hide1.js");
    let globals = Globals::new();
    GLOBALS.set(&globals, || {
        let code = parse(&path);
        std::fs::write("./output.js", code).unwrap();
    })
}

fn parse(filename: impl AsRef<Path>) -> String {
    let filename = filename.as_ref();
    let filename_text = filename.to_string_lossy().into_owned();
    let code = std::fs::read_to_string(filename).unwrap();

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
        |_| {
            as_folder(chain!(
                // resolver(unresolved_mark, top_level_mark, false),
                SeqExpandVisitor,
                VoidToUndefinedVisitor,
                NotLitVisitor,
                NotIifeVisitor,
                InitAssignmentVisitor,
                NestedAssignmentVisitor,
                VarDeclExpand,
                IifeExpandVisitor,
                // TODO: make toggleable
                EsModuleRenameVisitor,
            ))
        },
        |_| noop(),
    );

    handler.abort_if_errors();

    let TransformOutput { code, map: _ } = transformed.unwrap();

    code
}
