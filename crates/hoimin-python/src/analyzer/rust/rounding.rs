//! Precision edits change a rounding choice while retaining the evaluation
//! order of values, explicitly supplied options, and unrelated arithmetic.

use super::{
    AnalysisCancelled,
    call_contracts::evaluated_arguments,
    callable_imports::CallableImports,
    decimal_context::{Decimals, Quantize, arithmetic},
    decimal_quantum::{MAX_PRECISION, Quantum, signed_integer},
    processing_calls::{Site, bind},
};
use hoimin_core::MutationOperator;
use ruff_python_ast::token::parenthesized_range;
use ruff_python_ast::{Expr, ExprCall, ModModule, Stmt, UnaryOp};
use ruff_text_size::{Ranged, TextRange};

const SWAP: MutationOperator = MutationOperator::RoundingFunctionSwap;
const DIGITS: MutationOperator = MutationOperator::RoundDigitsRemove;
const MODE: MutationOperator = MutationOperator::DecimalRoundingModeSwap;
const QUANTUM: MutationOperator = MutationOperator::DecimalQuantumShift;
const PRECISION: MutationOperator = MutationOperator::DecimalContextPrecision;
const INTERMEDIATE: MutationOperator = MutationOperator::IntermediateRoundingRemove;

#[derive(Default)]
pub(super) struct Rounding {
    math: CallableImports,
    decimal: Decimals,
    active: bool,
    settings: bool,
}

impl Rounding {
    pub(super) fn build(
        module: &ModModule,
        site: &Site<'_>,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        let builtin = [SWAP, DIGITS, MODE, INTERMEDIATE]
            .into_iter()
            .any(|op| site.supports(op));
        let decimal = [MODE, QUANTUM, PRECISION, INTERMEDIATE]
            .into_iter()
            .any(|op| site.supports(op));
        Ok(Self {
            math: if site.supports(SWAP) {
                CallableImports::build_family(module, cancelled, &["math"])?
            } else {
                CallableImports::default()
            },
            decimal: if decimal {
                Decimals::build(module, cancelled)?
            } else {
                Decimals::default()
            },
            active: builtin || decimal,
            settings: site.supports(MODE) || site.supports(PRECISION),
        })
    }

    pub(super) fn visits_calls(&self) -> bool {
        self.active
    }

    pub(super) fn visits_assignments(&self) -> bool {
        self.settings
    }

    pub(super) fn emit_call(
        &self,
        call: &ExprCall,
        site: &Site<'_>,
        mut selected: impl FnMut(TextRange) -> bool,
        mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
    ) {
        if site.supports(SWAP) && !self.emit_swaps(call, site, &mut selected, &mut emit) {
            return;
        }
        let round = Self::round_arguments(call, site);
        if site.supports(DIGITS)
            && let Some([_, Some(digits)]) = round
            && !digits.is_none_literal_expr()
        {
            let range = site.argument_range(digits, call);
            if selected(range)
                && site.delimiters.can_enclose(range, 1)
                && let Some(text) = site.text(range)
                && ((site.cancelled)() || !emit(range, format!("({text}, None)[1]"), DIGITS))
            {
                return;
            }
        }
        if self.settings
            && let Some(settings) = self.decimal.context_arguments(call)
            && !self.emit_settings(
                settings.precision,
                settings.rounding,
                site,
                &mut selected,
                &mut emit,
            )
        {
            return;
        }
        let quantize = self.decimal.quantize(call, site.cancelled);
        if let Some(ref quantize) = quantize {
            if !self.emit_settings(None, quantize.rounding, site, &mut selected, &mut emit) {
                return;
            }
            if site.supports(QUANTUM)
                && let Some(quantum) = quantize.exp.as_call_expr()
                && let Some(literal) = self.decimal.decimal_argument(quantum)
                && Quantum::edit_range(literal).is_some_and(&mut selected)
                && let Some(quantum) = Quantum::from_literal(literal, site.cancelled)
                && !(site.cancelled)()
            {
                for direction in [-1, 1] {
                    if (site.cancelled)()
                        || quantum
                            .shifted(direction)
                            .is_some_and(|replacement| !emit(quantum.range, replacement, QUANTUM))
                    {
                        return;
                    }
                }
            }
        }
        if site.supports(INTERMEDIATE)
            && let Some(numeric) = round
                .and_then(|[number, _]| number)
                .or_else(|| quantize.map(|quantize| &*quantize.attribute.value))
        {
            self.emit_intermediate(numeric, site, &mut selected, &mut emit);
        }
    }

    pub(super) fn emit_assignment(
        &self,
        statement: &Stmt,
        site: &Site<'_>,
        mut selected: impl FnMut(TextRange) -> bool,
        mut emit: impl FnMut(TextRange, String, MutationOperator) -> bool,
    ) {
        let (target, value) = match statement {
            Stmt::Assign(assign) if assign.targets.len() == 1 => {
                (&assign.targets[0], &*assign.value)
            }
            Stmt::AnnAssign(assign) if assign.value.is_some() => (
                &*assign.target,
                assign.value.as_deref().expect("checked value"),
            ),
            _ => return,
        };
        if let Some(attribute) = target.as_attribute_expr()
            && matches!(attribute.attr.as_str(), "prec" | "rounding")
            && !(site.cancelled)()
            && self.decimal.context_target(attribute, site.cancelled)
        {
            let (precision, mode) = if attribute.attr.as_str() == "prec" {
                (Some(value), None)
            } else {
                (None, Some(value))
            };
            self.emit_settings(precision, mode, site, &mut selected, &mut emit);
        }
    }

    fn round_arguments<'a>(call: &'a ExprCall, site: &Site<'_>) -> Option<[Option<&'a Expr>; 2]> {
        if site.builtins.resolves(&call.func, "round", site) {
            bind(call, ["number", "ndigits"], 0, 2, 1)
        } else {
            None
        }
    }

    fn emit_swaps(
        &self,
        call: &ExprCall,
        site: &Site<'_>,
        selected: &mut impl FnMut(TextRange) -> bool,
        emit: &mut impl FnMut(TextRange, String, MutationOperator) -> bool,
    ) -> bool {
        if bind(call, ["x"], 1, 1, 1).is_none() {
            return true;
        }
        let origin = self.math.resolve(&call.func);
        let (original, member_range) = if let Some(source) = origin {
            if !matches!(source.member, "ceil" | "floor") {
                return true;
            }
            (source.member, source.member_range)
        } else if site.builtins.resolves(&call.func, "round", site) {
            ("round", None)
        } else {
            return true;
        };

        for destination in ["ceil", "floor", "round"] {
            if (site.cancelled)() {
                return false;
            }
            if destination == original {
                continue;
            }
            let range = if destination == "round" {
                None
            } else {
                member_range
            }
            .unwrap_or_else(|| call.func.range());
            if !selected(range) {
                continue;
            }
            let replacement = if destination == "round" {
                site.builtins
                    .destination("round", &call.func, site)
                    .then(|| "round".to_owned())
            } else if member_range.is_some() {
                Some(destination.to_owned())
            } else {
                site.builtins
                    .import_member(&self.math, "math", destination, &call.func, site)
            };
            if let Some(replacement) = replacement
                && !emit(range, replacement, SWAP)
            {
                return false;
            }
        }
        true
    }

    fn emit_settings(
        &self,
        precision: Option<&Expr>,
        mode: Option<&Expr>,
        site: &Site<'_>,
        selected: &mut impl FnMut(TextRange) -> bool,
        emit: &mut impl FnMut(TextRange, String, MutationOperator) -> bool,
    ) -> bool {
        if site.supports(PRECISION)
            && let Some(value) = precision
            && selected(value.range())
            && let Some(precision) = signed_integer(value)
            && (1..=MAX_PRECISION).contains(&precision)
        {
            let replacement = if precision >= 3 { 1 } else { precision + 2 };
            if (site.cancelled)() || !emit(value.range(), replacement.to_string(), PRECISION) {
                return false;
            }
        }
        if site.supports(MODE)
            && let Some(mode) = mode
            && let Some((range, replacement)) = self.mode_replacement(mode, site, selected)
            && ((site.cancelled)() || !emit(range, replacement, MODE))
        {
            return false;
        }
        true
    }

    fn mode_replacement(
        &self,
        expr: &Expr,
        site: &Site<'_>,
        selected: &mut impl FnMut(TextRange) -> bool,
    ) -> Option<(TextRange, String)> {
        if let Expr::StringLiteral(literal) = expr {
            let range = expr.range();
            if !selected(range) {
                return None;
            }
            let mut text = String::new();
            for part in &literal.value {
                if (site.cancelled)() || text.len() + part.value.len() > "ROUND_HALF_EVEN".len() {
                    return None;
                }
                text.push_str(&part.value);
            }
            let mode = alternate_mode(&text)?;
            return Some((range, format!("'{mode}'")));
        }
        let constant = self.decimal.imports.resolve(expr)?;
        let range = constant.member_range.unwrap_or_else(|| expr.range());
        if !selected(range) {
            return None;
        }
        let mode = alternate_mode(constant.member)?;
        let replacement = if constant.member_range.is_some() {
            mode.to_owned()
        } else {
            site.builtins
                .import_member(&self.decimal.imports, "decimal", mode, expr, site)?
        };
        Some((range, replacement))
    }

    fn emit_intermediate(
        &self,
        numeric: &Expr,
        site: &Site<'_>,
        selected: &mut impl FnMut(TextRange) -> bool,
        emit: &mut impl FnMut(TextRange, String, MutationOperator) -> bool,
    ) {
        let mut pending = Vec::new();
        if !push_operands(numeric, &mut pending) {
            return;
        }
        while let Some(expr) = pending.pop() {
            if (site.cancelled)() {
                return;
            }
            if push_operands(expr, &mut pending) {
                continue;
            }
            let range = expr.range();
            if let Some(call) = expr.as_call_expr()
                && selected(range)
                && let Some(replacement) = self.without_rounding(call, site)
                && ((site.cancelled)() || !emit(range, replacement, INTERMEDIATE))
            {
                return;
            }
        }
    }

    fn without_rounding(&self, call: &ExprCall, site: &Site<'_>) -> Option<String> {
        if let Some([Some(number), _]) = Self::round_arguments(call, site) {
            let index = call
                .arguments
                .args
                .iter()
                .chain(call.arguments.keywords.iter().map(|keyword| &keyword.value))
                .position(|value| std::ptr::eq(value, number))?;
            return evaluated_arguments(
                call,
                index,
                None,
                site.source,
                site.tokens(),
                site.cancelled,
            );
        }
        let Quantize { attribute, .. } = self.decimal.quantize(call, site.cancelled)?;
        let receiver = &attribute.value;
        let range = parenthesized_range(receiver.as_ref().into(), attribute.into(), site.tokens())
            .unwrap_or_else(|| receiver.range());
        if !site.delimiters.can_enclose(range, 1) {
            return None;
        }
        evaluated_arguments(
            call,
            0,
            Some(site.text(range)?),
            site.source,
            site.tokens(),
            site.cancelled,
        )
    }
}

fn push_operands<'a>(expr: &'a Expr, pending: &mut Vec<&'a Expr>) -> bool {
    match expr {
        Expr::BinOp(binary) if arithmetic(binary.op) => {
            pending.push(&binary.right);
            pending.push(&binary.left);
            true
        }
        Expr::UnaryOp(unary) if matches!(unary.op, UnaryOp::UAdd | UnaryOp::USub) => {
            pending.push(&unary.operand);
            true
        }
        _ => false,
    }
}

fn alternate_mode(mode: &str) -> Option<&'static str> {
    Some(match mode {
        "ROUND_HALF_EVEN" | "ROUND_HALF_DOWN" => "ROUND_HALF_UP",
        "ROUND_HALF_UP" => "ROUND_HALF_EVEN",
        "ROUND_CEILING" => "ROUND_FLOOR",
        "ROUND_FLOOR" => "ROUND_CEILING",
        "ROUND_DOWN" | "ROUND_05UP" => "ROUND_UP",
        "ROUND_UP" => "ROUND_DOWN",
        _ => return None,
    })
}
