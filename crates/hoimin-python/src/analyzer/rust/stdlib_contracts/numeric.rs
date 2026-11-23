//! Bind each numerical API before choosing a tolerance, arithmetic expression
//! or sibling callable. Source splices retain argument order and lexical scope.
use super::super::{callable_imports::Destination, processing_calls::implicit_generator_interior};
use super::{AnalysisCancelled, CallableImports, Site, bind, options::insert_keyword};
use hoimin_core::{MutationOperator as Op, MutationOperatorSelection};
use ruff_python_ast::{Expr, ExprCall, ModModule, Number, UnaryOp};
use ruff_text_size::{Ranged, TextRange, TextSize};

#[derive(Default)]
pub(super) struct Imports {
    math: CallableImports,
    statistics: CallableImports,
    heapq: CallableImports,
}

impl Imports {
    pub(super) fn build(
        module: &ModModule,
        operators: &MutationOperatorSelection,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        let family = |enabled, name: &'static [&'static str; 1]| {
            if enabled {
                CallableImports::build_family(module, cancelled, name)
            } else {
                Ok(CallableImports::default())
            }
        };
        Ok(Self {
            math: family(
                operators.contains(Op::MathIscloseTolerance)
                    || operators.contains(Op::MathPrecisionLoss),
                &["math"],
            )?,
            statistics: family(
                operators.contains(Op::StatisticsSamplePopulation),
                &["statistics"],
            )?,
            heapq: family(operators.contains(Op::HeapPushpopReplace), &["heapq"])?,
        })
    }

    pub(super) fn emit(
        &self,
        call: &ExprCall,
        site: &Site<'_>,
        emit: &mut impl FnMut(TextRange, TextSize, Op, &dyn Fn() -> String) -> bool,
    ) -> bool {
        let Some((imports, id)) = [&self.math, &self.statistics, &self.heapq]
            .into_iter()
            .find_map(|imports| imports.resolve(&call.func).map(|id| (imports, id)))
        else {
            return true;
        };
        let Some(api) = Api::recognize(id.module, id.member) else {
            return true;
        };
        let op = api.operator();
        if !site.supports(op) {
            return true;
        }
        if matches!(api, Api::Isclose) {
            return isclose(call, site, emit);
        }
        let Some(Sibling { member, center }) = api.bind_sibling(call) else {
            return true;
        };
        let (range, dest) = if let Some(range) = id.member_range {
            (range, Destination::Name(member))
        } else if let Some(dest) = site
            .builtins
            .import_destination(imports, id.module, member, &call.func, site)
        {
            (call.func.range(), dest)
        } else {
            return true;
        };
        match api {
            Api::Expm1 => {
                // The enclosing subtraction and an import fallback can nest
                // at the callee while the argument list is still unopened.
                if site.delimiters.can_enclose(call.range(), 1)
                    && (!matches!(dest, Destination::Import(..))
                        || site.delimiters.room_before(range.start(), 2))
                {
                    return emit(call.range(), call.start(), op, &|| {
                        let before = site
                            .text(TextRange::new(call.start(), range.start()))
                            .unwrap();
                        let after = site.text(TextRange::new(range.end(), call.end())).unwrap();
                        format!("({before}{dest}{after} - 1)")
                    });
                }
            }
            Api::Log1p => {
                let arg = implicit_generator_interior(call)
                    .unwrap_or_else(|| site.argument_range(&call.arguments.args[0], call));
                if site.delimiters.can_enclose(arg, 1) {
                    let replacement_span = TextRange::new(range.start(), arg.end());
                    return emit(replacement_span, range.start(), op, &|| {
                        let between = site.text(TextRange::new(range.end(), arg.start())).unwrap();
                        let value = site.text(arg).unwrap();
                        format!("{dest}{between}1 + ({value})")
                    });
                }
            }
            _ => {
                if let Some((kw, name)) = center {
                    let replacement_span = TextRange::new(range.start(), kw.end());
                    return emit(replacement_span, range.start(), op, &|| {
                        let between = site.text(TextRange::new(range.end(), kw.start())).unwrap();
                        format!("{dest}{between}{name}")
                    });
                }
                return emit(range, range.start(), op, &|| dest.to_string());
            }
        }
        true
    }
}

/// A center keyword can require a second spelling change in the same candidate.
struct Sibling {
    member: &'static str,
    center: Option<(TextRange, &'static str)>,
}

#[derive(Clone, Copy)]
enum Center {
    Sample,
    Population,
}

impl Center {
    fn names(self) -> (&'static str, &'static str) {
        match self {
            Self::Sample => ("xbar", "mu"),
            Self::Population => ("mu", "xbar"),
        }
    }
}

enum Api {
    Isclose,
    Expm1,
    Log1p,
    Statistics {
        center: Center,
        destination: &'static str,
    },
    Heap(&'static str),
}

impl Api {
    fn recognize(module: &str, member: &str) -> Option<Self> {
        Some(match (module, member) {
            ("math", "isclose") => Self::Isclose,
            ("math", "expm1") => Self::Expm1,
            ("math", "log1p") => Self::Log1p,
            ("statistics", "variance") => Self::Statistics {
                center: Center::Sample,
                destination: "pvariance",
            },
            ("statistics", "pvariance") => Self::Statistics {
                center: Center::Population,
                destination: "variance",
            },
            ("statistics", "stdev") => Self::Statistics {
                center: Center::Sample,
                destination: "pstdev",
            },
            ("statistics", "pstdev") => Self::Statistics {
                center: Center::Population,
                destination: "stdev",
            },
            ("heapq", "heappushpop") => Self::Heap("heapreplace"),
            ("heapq", "heapreplace") => Self::Heap("heappushpop"),
            _ => return None,
        })
    }

    fn bind_sibling(&self, call: &ExprCall) -> Option<Sibling> {
        let member = match self {
            Self::Isclose => return None,
            Self::Expm1 | Self::Log1p => {
                bind(call, ["x"], 1, 1, 1)?;
                if matches!(self, Self::Expm1) {
                    "exp"
                } else {
                    "log"
                }
            }
            Self::Heap(member) => {
                bind(call, ["heap", "item"], 2, 2, 2)?;
                member
            }
            Self::Statistics {
                center,
                destination,
            } => {
                let (original, replacement) = center.names();
                bind(call, ["data", original], 0, 2, 1)?;
                let keyword = call
                    .arguments
                    .keywords
                    .iter()
                    .filter_map(|kw| kw.arg.as_ref())
                    .find(|name| name.as_str() == original);
                return Some(Sibling {
                    member: destination,
                    center: keyword.map(|name| (name.range(), replacement)),
                });
            }
        };
        Some(Sibling {
            member,
            center: None,
        })
    }

    fn operator(&self) -> Op {
        match self {
            Self::Isclose => Op::MathIscloseTolerance,
            Self::Expm1 | Self::Log1p => Op::MathPrecisionLoss,
            Self::Statistics { .. } => Op::StatisticsSamplePopulation,
            Self::Heap(_) => Op::HeapPushpopReplace,
        }
    }
}

fn isclose(
    call: &ExprCall,
    site: &Site<'_>,
    emit: &mut impl FnMut(TextRange, TextSize, Op, &dyn Fn() -> String) -> bool,
) -> bool {
    let op = Op::MathIscloseTolerance;
    let Some([_, _, relative, absolute]) = bind(call, ["a", "b", "rel_tol", "abs_tol"], 0, 2, 2)
    else {
        return true;
    };
    if relative.is_none() && !insert_keyword(call, site, op, "rel_tol", "0.0", emit) {
        return false;
    }
    for value in [relative, absolute].into_iter().flatten() {
        let mut atom = value;
        while let Expr::UnaryOp(sign) = atom {
            if (site.cancelled)() {
                return false;
            }
            if !matches!(sign.op, UnaryOp::UAdd | UnaryOp::USub) {
                break;
            }
            atom = &sign.operand;
        }
        if (site.cancelled)() {
            return false;
        }
        let zero = match atom {
            Expr::BooleanLiteral(boolean) => Some(!boolean.value),
            Expr::NumberLiteral(literal) => match &literal.value {
                Number::Int(integer) => Some(integer.as_u64() == Some(0)),
                Number::Float(float) => Some(*float == 0.0),
                Number::Complex { .. } => None,
            },
            _ => None,
        };
        let range = value.range();
        if zero == Some(true) {
            continue;
        }
        // A signed atom may contain grouping and comments between the sign
        // and the literal. Only a bare literal can be discarded wholesale.
        if zero == Some(false) && !value.is_unary_op_expr() {
            if !emit(range, range.start(), op, &|| "0.0".into()) {
                return false;
            }
        } else if site.delimiters.can_enclose(range, 2)
            && !emit(range, range.start(), op, &|| {
                let source = site.text(range).unwrap();
                format!("(({source}), 0.0)[1]")
            })
        {
            return false;
        }
    }
    true
}
