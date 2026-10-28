//! Late reads for simple loop captures while preserving explicit callback arguments.
use super::AnalysisCancelled;
use crate::visitor::{self, Visitor};
use ruff_python_ast::{Expr, ExprLambda, ModModule, Stmt};
use ruff_text_size::{Ranged, TextRange};

#[derive(Clone, Copy, Eq, PartialEq)]
enum Flow {
    Running,
    Stopped,
    Cancelled,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CaptureNamespace {
    Ordinary,
    Class,
}

struct LoopBinding<'a> {
    target: &'a Expr,
    ordinary: bool,
}

pub(super) fn emit(
    module: &ModModule,
    source: &str,
    cancelled: &impl Fn() -> bool,
    selected: impl FnMut(TextRange) -> bool,
    emit: impl FnMut(TextRange, String) -> bool,
) -> Result<(), AnalysisCancelled> {
    let mut scan = Captures {
        source,
        cancelled,
        selected,
        emit,
        loops: Vec::new(),
        flow: Flow::Running,
        namespace: CaptureNamespace::Ordinary,
    };
    scan.visit_body(&module.body);
    if scan.flow == Flow::Cancelled {
        Err(AnalysisCancelled)
    } else {
        Ok(())
    }
}

struct Captures<'a, 'source, F, S, E> {
    source: &'source str,
    cancelled: &'source F,
    selected: S,
    emit: E,
    loops: Vec<LoopBinding<'a>>,
    flow: Flow,
    namespace: CaptureNamespace,
}

impl<F: Fn() -> bool, S: FnMut(TextRange) -> bool, E: FnMut(TextRange, String) -> bool>
    Captures<'_, '_, F, S, E>
{
    fn active(&mut self) -> bool {
        if self.flow != Flow::Running {
            return false;
        }
        if (self.cancelled)() {
            self.flow = Flow::Cancelled;
            return false;
        }
        true
    }

    fn collect(&mut self, lambda: &ExprLambda) {
        let Some(parameters) = &lambda.parameters else {
            return;
        };
        let [parameter] = parameters.args.as_slice() else {
            return;
        };
        if !parameters.posonlyargs.is_empty()
            || !parameters.kwonlyargs.is_empty()
            || parameters.vararg.is_some()
            || parameters.kwarg.is_some()
        {
            return;
        }
        let Some(Expr::Name(default)) = parameter.default.as_deref() else {
            return;
        };
        let Expr::Name(body) = lambda.body.as_ref() else {
            return;
        };
        let name = parameter.parameter.name.as_str();
        if default.id.as_str() != name || body.id.as_str() != name {
            return;
        }
        let mut matching = None;
        for binding in self.loops.iter().rev() {
            match binds(binding.target, name, self.cancelled) {
                Ok(true) => {
                    matching = Some(binding);
                    break;
                }
                Ok(false) => {}
                Err(_) => {
                    self.flow = Flow::Cancelled;
                    return;
                }
            }
        }
        let Some(binding) = matching else {
            return;
        };
        if !binding.ordinary
            || !matches!(binding.target, Expr::Name(_))
            || !(self.selected)(lambda.range())
        {
            return;
        }
        if !self.active() {
            return;
        }
        let mut getter = "_hoimin_get".to_owned();
        if getter == name {
            getter.push('_');
        }
        let mut missing = "_hoimin_missing".to_owned();
        if missing == name {
            missing.push('_');
        }
        let start = usize::from(lambda.start());
        let mut edited = self.source[start..usize::from(lambda.end())].to_owned();
        // Edit right-to-left, leaving original grouping, whitespace and comments.
        edited.replace_range(
            usize::from(body.start()) - start..usize::from(body.end()) - start,
            &format!("({getter}() if {name} is {missing} else {name})"),
        );
        edited.replace_range(
            usize::from(default.start()) - start..usize::from(default.end()) - start,
            &missing,
        );
        let replacement = format!("(lambda {getter}, {missing}: {edited})(lambda: {name}, [])");
        if !self.active() {
            return;
        }
        if !(self.emit)(lambda.range(), replacement) {
            self.flow = Flow::Stopped;
        }
    }
}

fn binds(
    target: &Expr,
    name: &str,
    cancelled: &impl Fn() -> bool,
) -> Result<bool, AnalysisCancelled> {
    if cancelled() {
        return Err(AnalysisCancelled);
    }
    match target {
        Expr::Name(value) => Ok(value.id.as_str() == name),
        Expr::Tuple(value) => binds_elements(&value.elts, name, cancelled),
        Expr::List(value) => binds_elements(&value.elts, name, cancelled),
        Expr::Starred(value) => binds(&value.value, name, cancelled),
        _ => Ok(false),
    }
}

fn binds_elements(
    elements: &[Expr],
    name: &str,
    cancelled: &impl Fn() -> bool,
) -> Result<bool, AnalysisCancelled> {
    for element in elements {
        if binds(element, name, cancelled)? {
            return Ok(true);
        }
    }
    Ok(false)
}

impl<'a, F: Fn() -> bool, S: FnMut(TextRange) -> bool, E: FnMut(TextRange, String) -> bool>
    Visitor<'a> for Captures<'a, '_, F, S, E>
{
    fn visit_stmt(&mut self, statement: &'a Stmt) {
        if !self.active() {
            return;
        }
        match statement {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {
                let previous = std::mem::take(&mut self.loops);
                let previous_namespace = self.namespace;
                self.namespace = if matches!(statement, Stmt::ClassDef(_)) {
                    CaptureNamespace::Class
                } else {
                    CaptureNamespace::Ordinary
                };
                visitor::walk_stmt(self, statement);
                self.loops = previous;
                self.namespace = previous_namespace;
            }
            Stmt::For(value) => {
                self.visit_expr(&value.iter);
                self.visit_expr(&value.target);
                self.loops.push(LoopBinding {
                    target: &value.target,
                    ordinary: !value.is_async && self.namespace == CaptureNamespace::Ordinary,
                });
                self.visit_body(&value.body);
                self.loops.pop();
                self.visit_body(&value.orelse);
            }
            Stmt::TypeAlias(_) => {}
            _ => visitor::walk_stmt(self, statement),
        }
    }
    fn visit_expr(&mut self, expression: &'a Expr) {
        if !self.active() {
            return;
        }
        match expression {
            Expr::Lambda(value) => self.collect(value),
            Expr::ListComp(_) | Expr::SetComp(_) | Expr::DictComp(_) | Expr::Generator(_) => {}
            _ => visitor::walk_expr(self, expression),
        }
    }
}
