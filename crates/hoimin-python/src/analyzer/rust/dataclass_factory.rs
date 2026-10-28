//! Share the empty container produced by a builtin dataclass field factory.
//!
//! The first version accepts direct top-level dataclasses whose suites consist
//! only of annotated name fields, pass statements and string literals. Ordinary
//! import resolution retains its stricter rejection of all class loads.
use super::{
    AnalysisCancelled, NameResolution, NameResolutionIndex, callable_imports::CallableImports,
};
use hoimin_core::{MutationOperator, MutationOperatorSelection};
use ruff_python_ast::{Expr, ModModule, Stmt, StmtClassDef};
use ruff_text_size::{Ranged, TextRange};
use std::collections::HashSet;

#[derive(Default)]
pub(super) struct DataclassFactories {
    imports: CallableImports,
    top_level: HashSet<TextRange>,
}

impl DataclassFactories {
    pub(super) fn build(
        module: &ModModule,
        operators: &MutationOperatorSelection,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        if !operators.contains(MutationOperator::DataclassFactoryShare) {
            return Ok(Self::default());
        }
        let mut top_level = HashSet::new();
        for statement in &module.body {
            if cancelled() {
                return Err(AnalysisCancelled);
            }
            if let Stmt::ClassDef(class) = statement {
                top_level.insert(class.range());
            }
        }
        if top_level.is_empty() {
            return Ok(Self::default());
        }
        Ok(Self {
            imports: CallableImports::build(module, cancelled, &["dataclasses"])?,
            top_level,
        })
    }

    pub(super) fn emit_class(
        &self,
        class: &StmtClassDef,
        resolution: &NameResolutionIndex,
        cancelled: &impl Fn() -> bool,
        mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
    ) -> Result<(), AnalysisCancelled> {
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        if !self.top_level.contains(&class.range()) {
            return Ok(());
        }
        let Some(namespace) = self
            .imports
            .ordinary_dataclass_namespace(class, cancelled)?
        else {
            return Ok(());
        };
        for statement in &class.body {
            if cancelled() {
                return Err(AnalysisCancelled);
            }
            let Stmt::AnnAssign(assignment) = statement else {
                continue;
            };
            let Some(Expr::Call(call)) = assignment.value.as_deref() else {
                continue;
            };
            if !call.arguments.args.is_empty()
                || self
                    .imports
                    .resolve_in_ordinary_dataclass(&call.func, &namespace)
                    .is_none()
            {
                continue;
            }
            let mut factory = None;
            let mut uncertain = false;
            for keyword in &call.arguments.keywords {
                if cancelled() {
                    return Err(AnalysisCancelled);
                }
                match keyword
                    .arg
                    .as_ref()
                    .map(ruff_python_ast::Identifier::as_str)
                {
                    None | Some("default") => {
                        uncertain = true;
                        break;
                    }
                    Some("default_factory") if factory.replace(&keyword.value).is_some() => {
                        uncertain = true;
                        break;
                    }
                    _ => {}
                }
            }
            if uncertain {
                continue;
            }
            let Some(Expr::Name(factory)) = factory else {
                continue;
            };
            if namespace.binds(factory.id.as_str())
                || resolution.resolution(usize::from(factory.start()), factory.id.as_str())
                    != NameResolution::DefinitelyBuiltin
            {
                continue;
            }
            let replacement = match factory.id.as_str() {
                "list" => "lambda shared=[]: shared",
                "dict" => "lambda shared={}: shared",
                "set" => "lambda shared={*()}: shared",
                _ => continue,
            };
            if !emit(
                factory.range(),
                replacement.to_owned(),
                MutationOperator::DataclassFactoryShare,
            ) {
                return Ok(());
            }
        }
        Ok(())
    }
}
