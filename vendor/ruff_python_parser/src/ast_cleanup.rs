//! Iterative destruction of owned parser trees, including speculative results.
//!
//! Local hoimin extension: recursive grammar parsing uses stacker, but derived
//! AST Drop does not. Detach every recursive edge before dropping a node shell.
use ruff_python_ast::visitor::transformer::{self, Transformer};
use ruff_python_ast::{
    AtomicNodeIndex, Expr, ExprNoneLiteral, InterpolatedStringElement,
    InterpolatedStringLiteralElement, ModModule, Pattern, PatternMatchStar, Stmt, StmtPass,
};
use ruff_text_size::TextRange;
use std::cell::RefCell;

enum Node {
    Statement(Stmt),
    Expression(Expr),
    Pattern(Pattern),
    Element(InterpolatedStringElement),
}
struct Detach(RefCell<Vec<Node>>);
impl Transformer for Detach {
    fn visit_stmt(&self, node: &mut Stmt) {
        let empty = Stmt::Pass(StmtPass {
            node_index: AtomicNodeIndex::default(),
            range: TextRange::default(),
        });
        self.0
            .borrow_mut()
            .push(Node::Statement(std::mem::replace(node, empty)));
    }
    fn visit_expr(&self, node: &mut Expr) {
        let empty = Expr::NoneLiteral(ExprNoneLiteral::default());
        self.0
            .borrow_mut()
            .push(Node::Expression(std::mem::replace(node, empty)));
    }
    fn visit_pattern(&self, node: &mut Pattern) {
        let empty = Pattern::MatchStar(PatternMatchStar {
            node_index: AtomicNodeIndex::default(),
            range: TextRange::default(),
            name: None,
        });
        self.0
            .borrow_mut()
            .push(Node::Pattern(std::mem::replace(node, empty)));
    }
    fn visit_interpolated_string_element(&self, node: &mut InterpolatedStringElement) {
        let empty = InterpolatedStringElement::Literal(InterpolatedStringLiteralElement {
            node_index: AtomicNodeIndex::default(),
            range: TextRange::default(),
            value: Box::default(),
        });
        self.0
            .borrow_mut()
            .push(Node::Element(std::mem::replace(node, empty)));
    }
}
/// Destroy a module without recursive AST destruction.
pub fn dispose_module(mut module: ModModule) {
    dispose(module.body.drain(..).map(Node::Statement).collect());
}

/// Destroy an expression discarded during parser recovery or speculation.
pub fn dispose_expression(expression: Expr) {
    dispose(vec![Node::Expression(expression)]);
}

/// Destroy a pattern discarded during syntax recovery.
pub(crate) fn dispose_pattern(pattern: Pattern) {
    dispose(vec![Node::Pattern(pattern)]);
}

fn dispose(nodes: Vec<Node>) {
    let detach = Detach(RefCell::new(nodes));
    loop {
        let Some(mut node) = detach.0.borrow_mut().pop() else {
            break;
        };
        match &mut node {
            Node::Statement(stmt) => transformer::walk_stmt(&detach, stmt),
            Node::Expression(expr) => transformer::walk_expr(&detach, expr),
            Node::Pattern(pattern) => transformer::walk_pattern(&detach, pattern),
            Node::Element(element) => {
                transformer::walk_interpolated_string_element(&detach, element);
            }
        }
        // All recursive children now belong to the worklist. Only shallow shells drop.
    }
}
