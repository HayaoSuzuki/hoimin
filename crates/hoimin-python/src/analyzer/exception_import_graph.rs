//! Shared eager-import cycle membership and conservative import-depth bounds.
use super::{AnalysisCancelled, BTreeMap, MAX_STEPS, Module, Utf8PathBuf};

macro_rules! checkpoint {
    ($cancelled:expr, $phase:ident) => {
        if $cancelled() {
            return Err(AnalysisCancelled);
        }
        #[cfg(test)]
        super::exception_import_graph_tests::record(
            super::exception_import_graph_tests::Phase::$phase,
        );
    };
}

/// Preserve existing dynamic flags and exclude actual cycle members or modules
/// whose SCC-weighted import depth reaches 256. Short prefixes entering cycles
/// remain eligible. Only present, mapped, absolute eager dependencies are edges;
/// already dynamic modules still participate. All graph scratch is temporary.
///
/// Cancellation returns no usable index to the caller. Graph visits/storage are
/// linear in vertices plus edges; ordered name/path lookup costs are additional.
pub(super) fn exclude(
    modules: &mut BTreeMap<Utf8PathBuf, Module>,
    mappings: &BTreeMap<String, Utf8PathBuf>,
    cancelled: &impl Fn() -> bool,
) -> Result<(), AnalysisCancelled> {
    let excluded =
        Graph::from_modules(modules, mappings, cancelled)?.excluded(MAX_STEPS, cancelled)?;
    for (module, excluded) in modules.values_mut().zip(excluded) {
        checkpoint!(cancelled, Apply);
        module.dynamic |= excluded;
    }
    Ok(())
}

struct Graph {
    forward: Vec<Vec<usize>>,
    reverse: Vec<Vec<usize>>,
    self_edges: Vec<bool>,
}
impl Graph {
    fn new(size: usize, cancelled: &impl Fn() -> bool) -> Result<Self, AnalysisCancelled> {
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        let mut graph = Self {
            forward: Vec::with_capacity(size),
            reverse: Vec::with_capacity(size),
            self_edges: Vec::with_capacity(size),
        };
        for _ in 0..size {
            checkpoint!(cancelled, Init);
            graph.forward.push(Vec::new());
            graph.reverse.push(Vec::new());
            graph.self_edges.push(false);
        }
        Ok(graph)
    }

    fn from_modules(
        modules: &BTreeMap<Utf8PathBuf, Module>,
        mappings: &BTreeMap<String, Utf8PathBuf>,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Self, AnalysisCancelled> {
        let mut graph = Self::new(modules.len(), cancelled)?;
        let mut ids = BTreeMap::new();
        for (id, path) in modules.keys().enumerate() {
            checkpoint!(cancelled, Init);
            ids.insert(path, id);
        }
        for (from, module) in modules.values().enumerate() {
            checkpoint!(cancelled, Init);
            // Poll every dependency, including lazy and unmapped entries.
            for ((name, level), usage) in &module.dependencies {
                checkpoint!(cancelled, Dependency);
                if *level == 0
                    && usage.eager
                    && let Some(path) = mappings.get(name)
                    && let Some(&to) = ids.get(path)
                {
                    checkpoint!(cancelled, EdgeBuild);
                    graph.edge(from, to);
                }
            }
        }
        Ok(graph)
    }

    fn edge(&mut self, from: usize, to: usize) {
        self.forward[from].push(to);
        self.reverse[to].push(from);
        self.self_edges[from] |= from == to;
    }

    fn finishing_order(
        &self,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Vec<usize>, AnalysisCancelled> {
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        let mut visited = vec![false; self.forward.len()];
        let mut order = Vec::with_capacity(visited.len());
        let mut stack = Vec::new();
        for root in 0..visited.len() {
            checkpoint!(cancelled, ForwardRoot);
            if visited[root] {
                continue;
            }
            visited[root] = true;
            stack.push((root, 0));
            while let Some((node, offset)) = stack.last_mut() {
                // One event for each edge examination or completed frame.
                checkpoint!(cancelled, Forward);
                if let Some(&next) = self.forward[*node].get(*offset) {
                    *offset += 1;
                    if !visited[next] {
                        visited[next] = true;
                        stack.push((next, 0));
                    }
                } else {
                    order.push(*node);
                    stack.pop();
                }
            }
        }
        Ok(order)
    }

    fn components(
        &self,
        order: Vec<usize>,
        cancelled: &impl Fn() -> bool,
    ) -> Result<(Vec<usize>, Vec<Vec<usize>>), AnalysisCancelled> {
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        let mut component_of = vec![usize::MAX; self.forward.len()];
        let mut components = Vec::new();
        let mut stack = Vec::new();
        for root in order.into_iter().rev() {
            checkpoint!(cancelled, ReverseRoot);
            if component_of[root] != usize::MAX {
                continue;
            }
            let component = components.len();
            let mut members = Vec::new();
            component_of[root] = component;
            stack.push(root);
            while let Some(node) = stack.pop() {
                checkpoint!(cancelled, ReverseNode);
                members.push(node);
                for &next in &self.reverse[node] {
                    checkpoint!(cancelled, ReverseEdge);
                    if component_of[next] == usize::MAX {
                        component_of[next] = component;
                        stack.push(next);
                    }
                }
            }
            components.push(members);
        }
        Ok((component_of, components))
    }

    fn excluded(
        &self,
        limit: usize,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Vec<bool>, AnalysisCancelled> {
        let order = self.finishing_order(cancelled)?;
        let (component_of, components) = self.components(order, cancelled)?;
        if cancelled() {
            return Err(AnalysisCancelled);
        }
        let mut heights = vec![0usize; components.len()];
        let mut rejected = vec![false; components.len()];
        for (id, members) in components.iter().enumerate().rev() {
            checkpoint!(cancelled, DepthComponent);
            let cyclic = members.len() > 1 || self.self_edges[members[0]];
            let weight = members.len().min(limit);
            let mut height = if cyclic { weight } else { 0 };
            for &node in members {
                checkpoint!(cancelled, DepthNode);
                for &next in &self.forward[node] {
                    checkpoint!(cancelled, DepthEdge);
                    let destination = component_of[next];
                    if destination != id {
                        // Kosaraju discovers SCCs in source-to-sink order. Every
                        // projected DAG edge therefore uses an already computed H.
                        debug_assert!(destination > id);
                        height = height.max(weight.saturating_add(heights[destination]).min(limit));
                    }
                }
            }
            heights[id] = height;
            rejected[id] = cyclic || height >= limit;
        }
        let mut excluded = Vec::with_capacity(component_of.len());
        for component in component_of {
            checkpoint!(cancelled, Classify);
            excluded.push(rejected[component]);
        }
        Ok(excluded)
    }
}

#[cfg(test)]
#[path = "exception_import_graph_properties.rs"]
mod properties;
