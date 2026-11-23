//! Integer position mutations and call options for statically resolved builtins.
use super::numeric_literals;
use super::processing_calls::{Site, implicit_generator_interior};
use hoimin_core::MutationOperator;
use ruff_python_ast::token::{TokenKind, parenthesized_range};
use ruff_python_ast::{AnyNodeRef, Expr, ExprCall, ExprSubscript};
use ruff_text_size::{Ranged, TextRange, TextSize};

pub(super) const INDEX_HELPERS: [&str; 3] = ["type", "range", "int"];

#[derive(Clone, Copy)]
enum Neighbor {
    Higher,
    Lower,
}

impl Neighbor {
    const BOTH: [Self; 2] = [Self::Higher, Self::Lower];

    fn shift(self, operand: &str, role: Position) -> String {
        let (delta, fallback) = match self {
            Self::Higher => ("+ 1", "-1"),
            Self::Lower => ("- 1", "1"),
        };
        match role {
            Position::Offset => format!("{operand} {delta}"),
            // A neighboring zero stride is invalid. Retain the original unit
            // stride in that direction without converting the operand twice.
            Position::Stride => format!("{operand} {delta} or {fallback}"),
        }
    }
}

#[derive(Clone, Copy)]
enum Position {
    Offset,
    Stride,
}

#[derive(Clone, Copy)]
enum RangeBound {
    Start,
    Stop,
    Step,
}

impl RangeBound {
    fn shifted_fields(self, neighbor: Neighbor) -> String {
        let mut fields = ["r.start", "r.stop", "r.step"];
        let (index, role) = match self {
            Self::Start => (0, Position::Offset),
            Self::Stop => (1, Position::Offset),
            Self::Step => (2, Position::Stride),
        };
        let shifted = neighbor.shift(fields[index], role);
        fields[index] = &shifted;
        fields.join(", ")
    }
}

/// Emit call edits lazily through the collector's ordered payload admission.
pub(super) fn call(
    call: &ExprCall,
    site: &Site<'_>,
    mut emit: impl FnMut(TextRange, MutationOperator, &dyn Fn() -> String) -> bool,
) {
    if site.supports(MutationOperator::ZipStrictToggle)
        && site.builtins.resolves(&call.func, "zip", site)
    {
        zip_strict(call, site, &mut emit);
    }
    if site.supports(MutationOperator::RangeBoundNeighbor)
        && site.builtins.resolves(&call.func, "range", site)
    {
        range_neighbors(call, site, &mut emit);
    }
}

fn zip_strict(
    call: &ExprCall,
    site: &Site<'_>,
    emit: &mut impl FnMut(TextRange, MutationOperator, &dyn Fn() -> String) -> bool,
) {
    let operator = MutationOperator::ZipStrictToggle;
    match call.arguments.keywords.as_ref() {
        [] => {
            if let Some(interior) = implicit_generator_interior(call) {
                if site.delimiters.can_enclose(interior, 1)
                    && let Some(generator) = site.text(interior)
                {
                    emit(interior, operator, &|| {
                        format!("({generator}), strict=True")
                    });
                }
                return;
            }
            let range = call.arguments.range();
            let closing = range.end() - TextSize::new(1);
            let prefix = TextRange::new(range.start(), closing);
            let last = site
                .tokens()
                .in_range(prefix)
                .iter()
                .rev()
                .find(|token| (site.cancelled)() || !token.kind().is_trivia());
            if !(site.cancelled)()
                && let (Some(last), Some(text)) = (last, site.text(prefix))
            {
                let comma = if matches!(last.kind(), TokenKind::Comma | TokenKind::Lpar) {
                    ""
                } else {
                    ", "
                };
                emit(range, operator, &|| format!("{text}{comma}strict=True)"));
            }
        }
        [keyword] if keyword.arg.as_ref().is_some_and(|name| name == "strict") => {
            let range = keyword.value.range();
            // BooleanLiteral already generates the same behavior change.
            if !keyword.value.is_boolean_literal_expr()
                && site.delimiters.can_enclose(range, 2)
                && site.delimiters.room_before(range.start(), 5)
                && let Some(flag) = site.text(range)
            {
                // A literal tuple's metatype is the builtin type constructor.
                // Keep the original flag through iterator creation, but clear
                // its holder while testing truth so a traceback cannot keep a
                // failed temporary alive through the synthetic frame.
                emit(range, operator, &|| {
                    format!(
                        "().__class__.__class__('_HoiminStrict', (), {{\
                         '__slots__': ('flag',), \
                         '__init__': lambda self, value: self.__setattr__('flag', [value]), \
                         '__bool__': lambda self: self.flag.extend((self.flag[-1], not self.flag.pop())) \
                         or self.flag.pop()}})(({flag}))"
                    )
                });
            }
        }
        // Unknown/duplicate keywords cannot establish whether strict is omitted.
        _ => {}
    }
}

fn range_neighbors(
    call: &ExprCall,
    site: &Site<'_>,
    emit: &mut impl FnMut(TextRange, MutationOperator, &dyn Fn() -> String) -> bool,
) {
    let bounds: &[RangeBound] = match call.arguments.args.len() {
        1 => &[RangeBound::Stop],
        2 => &[RangeBound::Start, RangeBound::Stop],
        3 => &[RangeBound::Start, RangeBound::Stop, RangeBound::Step],
        _ => return,
    };
    let range = call.range();
    if !call.arguments.keywords.is_empty()
        || call.arguments.args.iter().any(Expr::is_starred_expr)
        || !site.delimiters.can_enclose(range, 1)
        || !site.delimiters.room_before(range.start(), 2)
    {
        return;
    }
    let Some(original) = site.text(range) else {
        return;
    };
    for &changed in bounds {
        for neighbor in Neighbor::BOTH {
            if (site.cancelled)()
                || !emit(range, MutationOperator::RangeBoundNeighbor, &|| {
                    let fields = changed.shifted_fields(neighbor);
                    // The original builtin call already evaluated and converted
                    // its operands in the right order. Its fields are exact ints.
                    format!("(lambda r: r.__class__({fields}))({original})")
                })
            {
                return;
            }
        }
    }
}

/// Emit load-subscript neighbors. Selection follows the original operand,
/// including when a computed edit must replace the entire subscript.
pub(super) fn subscript(
    subscript: &ExprSubscript,
    site: &Site<'_>,
    selected: impl Fn(TextRange) -> bool,
    mut emit: impl FnMut(TextRange, TextRange, MutationOperator, &dyn Fn() -> String) -> bool,
) {
    let index = MutationOperator::StructureIndexNeighbor;
    let slicing = MutationOperator::StructureSliceNeighbor;
    if !site.supports(index) && !site.supports(slicing) {
        return;
    }
    let mut computed = None;
    let mut position = |expression: &Expr, slot: Slot, operator| {
        let range = expression.range();
        if (site.cancelled)() {
            return false;
        }
        if !site.supports(operator) || !selected(range) {
            return true;
        }
        if expression.is_number_literal_expr() || numeric_literals::is_signed(expression) {
            let mut keep_going = true;
            if site.delimiters.can_enclose(range, 1) {
                numeric_literals::emit_index_neighbors(
                    expression,
                    site.source,
                    matches!(slot, Slot::Step),
                    &site.cancelled,
                    |span, replacement| {
                        keep_going = emit(span, span, operator, &|| replacement.clone());
                        keep_going
                    },
                );
            }
            return keep_going;
        }
        if statically_not_index(expression)
            || matches!(subscript.value.as_ref(), Expr::Dict(_) | Expr::DictComp(_))
        {
            return true;
        }
        let Some(computed) = computed
            .get_or_insert_with(|| ComputedSubscript::at(subscript, site))
            .as_ref()
        else {
            return true;
        };
        for neighbor in Neighbor::BOTH {
            if (site.cancelled)()
                || !emit(subscript.range(), range, operator, &|| {
                    computed.render(slot, neighbor)
                })
            {
                return false;
            }
        }
        true
    };
    match subscript.slice.as_ref() {
        Expr::Slice(slice) => {
            for (bound, slot) in [
                (slice.lower.as_deref(), Slot::Lower),
                (slice.upper.as_deref(), Slot::Upper),
                (slice.step.as_deref(), Slot::Step),
            ] {
                if bound.is_some_and(|bound| !position(bound, slot, slicing)) {
                    return;
                }
            }
        }
        value => {
            position(value, Slot::Index, index);
        }
    }
}

#[derive(Clone, Copy)]
enum Slot {
    Index,
    Lower,
    Upper,
    Step,
}

enum SubscriptKey<'a> {
    Index(&'a str),
    Slice([&'a str; 3]),
}

struct ComputedSubscript<'a> {
    receiver: &'a str,
    key: SubscriptKey<'a>,
    helpers: IndexHelpers,
}

impl<'a> ComputedSubscript<'a> {
    fn at(subscript: &ExprSubscript, site: &'a Site<'_>) -> Option<Self> {
        if !site.delimiters.room_before(subscript.start(), 6) {
            return None;
        }
        let source = |expression: &Expr, parent: AnyNodeRef<'_>, added| {
            let range = parenthesized_range(expression.into(), parent, site.tokens())
                .unwrap_or_else(|| expression.range());
            site.text(range)
                .filter(|_| site.delimiters.can_enclose(range, added))
        };
        let receiver = source(&subscript.value, subscript.into(), 3)?;
        let key = match subscript.slice.as_ref() {
            Expr::Slice(slice) => {
                let read = |bound: Option<&Expr>| match bound {
                    Some(bound) => source(bound, slice.into(), 2),
                    None => Some("None"),
                };
                SubscriptKey::Slice([
                    read(slice.lower.as_deref())?,
                    read(slice.upper.as_deref())?,
                    read(slice.step.as_deref())?,
                ])
            }
            value => SubscriptKey::Index(source(value, subscript.into(), 2)?),
        };
        let helpers = IndexHelpers::at(subscript, site)?;
        Some(Self {
            receiver,
            key,
            helpers,
        })
    }

    fn render(&self, slot: Slot, neighbor: Neighbor) -> String {
        let (body, arguments) = match (&self.key, slot) {
            (SubscriptKey::Index(index), Slot::Index) => (
                "adapt(held.pop(0), convert, integer, held.pop(0))",
                format!("({index})"),
            ),
            (SubscriptKey::Slice([lower, upper, step]), Slot::Lower | Slot::Upper | Slot::Step) => {
                let body = match slot {
                    Slot::Lower => {
                        "adapt(held.pop(0), convert, integer, held.pop(0)):held.pop(0):held.pop(0)"
                    }
                    Slot::Upper => {
                        "held.pop(1):adapt(held.pop(0), convert, integer, held.pop(0)):held.pop(0)"
                    }
                    Slot::Step => {
                        "held.pop(1):held.pop(1):adapt(held.pop(0), convert, integer, held.pop(0))"
                    }
                    Slot::Index => unreachable!("slice slot"),
                };
                (body, format!("({lower}), ({upper}), ({step})"))
            }
            _ => unreachable!("subscript shape matches selected slot"),
        };
        let role = if matches!(slot, Slot::Step) {
            Position::Stride
        } else {
            Position::Offset
        };
        let adapter = self.helpers.adapter(neighbor, role);
        let IndexHelpers {
            convert, integer, ..
        } = &self.helpers;
        let receiver = self.receiver;
        // Keep original expressions in caller scope. The operating frame
        // evacuates its holder before the subscription, so a traceback cannot
        // retain temporaries that the original would have released.
        format!(
            "(lambda held, convert, integer, adapt: held[0][{body}])([({receiver}), {arguments}], {convert}, {integer}, {adapter})"
        )
    }
}

fn statically_not_index(expression: &Expr) -> bool {
    matches!(
        expression,
        Expr::NoneLiteral(_)
            | Expr::EllipsisLiteral(_)
            | Expr::StringLiteral(_)
            | Expr::BytesLiteral(_)
            | Expr::FString(_)
            | Expr::TString(_)
            | Expr::Tuple(_)
            | Expr::List(_)
            | Expr::ListComp(_)
            | Expr::Set(_)
            | Expr::SetComp(_)
            | Expr::Dict(_)
            | Expr::DictComp(_)
            | Expr::Generator(_)
            | Expr::Lambda(_)
            | Expr::Starred(_)
    )
}

struct IndexHelpers {
    constructor: String,
    convert: String,
    integer: String,
}

impl IndexHelpers {
    fn at(subscript: &ExprSubscript, site: &Site<'_>) -> Option<Self> {
        let position = super::HelperSite::subscript(subscript);
        let [constructor, convert, integer] =
            INDEX_HELPERS.map(|name| site.builtins.generated_destination(name, position, site));
        Some(Self {
            constructor: constructor?,
            convert: convert?,
            integer: integer?,
        })
    }

    fn adapter(&self, neighbor: Neighbor, role: Position) -> String {
        let constructor = &self.constructor;
        let exact = neighbor.shift("convert(value).stop", role);
        // Clear only this invocation's receiver cell. A named expression
        // anywhere in a comprehension iterable is illegal, even in a lambda.
        let deferred = neighbor.shift(
            "self.convert((self.value, (lambda: self).__closure__[0].__setattr__('cell_contents', None))[0]).stop",
            role,
        );
        // Evaluate the source operand as an argument, outside the generated
        // lambdas. Exact integers remain integers for Python __getitem__.
        // For other objects, the receiver schedules __index__ as before (slice
        // conversion, for example, occurs after all bounds evaluate and starts
        // with the step). Keep the operand on the instance, so a cyclic heap
        // type cannot extend the operand's lifetime. Clear the frame's own
        // self reference before conversion so a traceback cannot keep the
        // operand alive. A receiver holding the proxy can still retry. Inspect
        // the real type dictionaries through builtin descriptors so arbitrary
        // mapping keys without the index protocol pass through without user lookups.
        format!(
            "{constructor}('_HoiminIndex', (), {{\
             '__slots__': ('value', 'convert'), \
             '__new__': lambda cls, receiver, convert, integer, value: value if value is None or \
             cls.mapping(cls.__class__, cls.__class__(receiver)) else \
             {exact} if cls.__class__(value) is integer or value is True or value is False else \
             cls.__mro__[-1].__new__(cls) if cls.indexable(cls.__class__, cls.__class__(value)) else value, \
             'mapping': lambda t, cls: [1 for base in t.__dict__['__mro__'].__get__(cls) \
             if base is {{}}.__class__ or base is t(t.__dict__)], \
             'indexable': lambda t, cls: [1 for base in t.__dict__['__mro__'].__get__(cls) \
             if '__index__' in t.__dict__['__dict__'].__get__(base)], \
             '__init__': lambda self, receiver, convert, integer, value: \
             self.__setattr__('value', value) or self.__setattr__('convert', convert), \
             '__index__': lambda self: {deferred}}})"
        )
    }
}
