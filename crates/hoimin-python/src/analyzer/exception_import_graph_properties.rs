use proptest::prelude::*;

use super::Graph;

fn graph(edges: &[Vec<usize>]) -> Graph {
    let mut graph = Graph::new(edges.len(), &|| false).unwrap();
    for (from, destinations) in edges.iter().enumerate() {
        for &to in destinations {
            graph.edge(from, to);
        }
    }
    graph
}

/// Independent small-graph oracle: positive transitive closure, equivalence
/// classes, then explicit enumeration/scoring of whole component paths.
fn reference(edges: &[Vec<usize>], limit: usize) -> Vec<bool> {
    let size = edges.len();
    let mut reachable = vec![vec![false; size]; size];
    for (from, destinations) in edges.iter().enumerate() {
        for &to in destinations {
            reachable[from][to] = true;
        }
    }
    for via in 0..size {
        for from in 0..size {
            for to in 0..size {
                reachable[from][to] |= reachable[from][via] && reachable[via][to];
            }
        }
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (vertex, reach) in reachable.iter().enumerate() {
        if let Some(group) = groups
            .iter_mut()
            .find(|g| reach[g[0]] && reachable[g[0]][vertex])
        {
            group.push(vertex);
        } else {
            groups.push(vec![vertex]);
        }
    }
    let group_of = (0..size)
        .map(|v| groups.iter().position(|g| g.contains(&v)).unwrap())
        .collect::<Vec<_>>();
    let mut quotient = vec![vec![false; groups.len()]; groups.len()];
    for (from, destinations) in edges.iter().enumerate() {
        for &to in destinations {
            if group_of[from] != group_of[to] {
                quotient[group_of[from]][group_of[to]] = true;
            }
        }
    }
    let mut result = vec![false; size];
    for start in 0..size {
        let mut paths = vec![vec![group_of[start]]];
        let mut largest = 0;
        while let Some(path) = paths.pop() {
            let last = *path.last().unwrap();
            let terminal = if reachable[groups[last][0]][groups[last][0]] {
                groups[last].len()
            } else {
                0
            };
            let score = path[..path.len() - 1]
                .iter()
                .map(|&g| groups[g].len())
                .sum::<usize>()
                + terminal;
            largest = largest.max(score);
            for (next, &connected) in quotient[last].iter().enumerate() {
                if connected {
                    assert!(!path.contains(&next), "quotient must be acyclic");
                    let mut extension = path.clone();
                    extension.push(next);
                    paths.push(extension);
                }
            }
        }
        result[start] = reachable[start][start] || largest >= limit;
    }
    result
}

fn old_exclusions(edges: &[Vec<usize>], limit: usize) -> Vec<bool> {
    (0..edges.len())
        .map(|start| {
            let mut visited = vec![false; edges.len()];
            let mut pending = vec![(start, 0)];
            let mut rejected = false;
            while let Some((node, depth)) = pending.pop() {
                if depth >= limit {
                    return true;
                }
                if visited[node] {
                    continue;
                }
                visited[node] = true;
                for &next in &edges[node] {
                    if next == start {
                        rejected = true;
                    } else {
                        pending.push((next, depth + 1));
                    }
                }
            }
            rejected
        })
        .collect()
}

fn small_graph() -> impl Strategy<Value = Vec<Vec<usize>>> {
    (0usize..8).prop_flat_map(|size| {
        prop::collection::vec(any::<bool>(), size * size).prop_map(move |bits| {
            (0..size)
                .map(|from| (0..size).filter(|&to| bits[from * size + to]).collect())
                .collect()
        })
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]
    #[test]
    fn shared_graph_matches_independent_path_reference(edges in small_graph(), limit in 1usize..9) {
        // Arrange.
        let expected = reference(&edges, limit);
        // Act.
        let actual = graph(&edges).excluded(limit, &||false).unwrap();
        // Assert.
        prop_assert_eq!(actual, expected);
    }

    #[test]
    fn every_old_exclusion_remains_excluded(edges in small_graph(), limit in 1usize..9, reversed in any::<bool>()) {
        // Arrange: the legacy predicate is separate from the H reference.
        let mut ordered = edges;
        if reversed { for destinations in &mut ordered { destinations.reverse(); } }
        let old = old_exclusions(&ordered, limit);
        // Act.
        let actual = graph(&ordered).excluded(limit, &||false).unwrap();
        // Assert.
        prop_assert!(old.iter().zip(actual).all(|(&before, after)| !before || after));
    }

    #[test]
    fn renaming_vertices_and_reversing_edges_preserves_exclusions(edges in small_graph(), keys in prop::collection::vec(any::<u8>(), 8), limit in 1usize..9) {
        // Arrange: a generated total ordering gives a bijection of vertex names.
        let mut order = (0..edges.len()).collect::<Vec<_>>();
        order.sort_by_key(|&v|(keys[v], v));
        let mut renamed = vec![Vec::new(); edges.len()];
        for (from, destinations) in edges.iter().enumerate() {
            renamed[order[from]] = destinations.iter().rev().map(|&to|order[to]).collect();
        }
        let expected = graph(&edges).excluded(limit, &||false).unwrap();
        // Act.
        let actual = graph(&renamed).excluded(limit, &||false).unwrap();
        // Assert.
        prop_assert!((0..edges.len()).all(|v|expected[v] == actual[order[v]]));
    }

    #[test]
    fn dag_depth_equals_enumerated_edge_paths(mut edges in small_graph(), limit in 1usize..9) {
        // Arrange: ascending edges enforce a DAG independently of the SCC algorithm.
        for (from, destinations) in edges.iter_mut().enumerate() { destinations.retain(|&to|to>from); }
        let expected = (0..edges.len()).map(|start| {
            let mut paths = vec![vec![start]];
            let mut longest = 0;
            while let Some(path) = paths.pop() {
                longest = longest.max(path.len()-1);
                for &next in &edges[*path.last().unwrap()] {
                    let mut extended = path.clone(); extended.push(next); paths.push(extended);
                }
            }
            longest >= limit
        }).collect::<Vec<_>>();
        // Act.
        let actual = graph(&edges).excluded(limit, &||false).unwrap();
        // Assert.
        prop_assert_eq!(actual, expected);
    }
}

macro_rules! graph_case {
    ($name:ident, $edges:expr, $limit:expr, $expected:expr) => {
        #[test]
        fn $name() {
            // Arrange.
            let input: Vec<Vec<usize>> = $edges;
            // Act.
            let excluded = graph(&input).excluded($limit, &|| false).unwrap();
            // Assert: literal expected vertices, independent of any oracle helper.
            assert_eq!(excluded, $expected);
        }
    };
}
graph_case!(empty_graph, vec![], 256, Vec::<bool>::new());
graph_case!(isolated_vertex, vec![vec![]], 256, vec![false]);
graph_case!(self_cycle, vec![vec![0]], 256, vec![true]);
graph_case!(mutual_cycle, vec![vec![1], vec![0]], 256, vec![true, true]);
graph_case!(
    diamond_is_not_cycle,
    vec![vec![1, 2], vec![3], vec![3], vec![]],
    256,
    vec![false; 4]
);
graph_case!(
    prefix_is_not_cycle_member,
    vec![vec![1], vec![2], vec![1]],
    256,
    vec![false, true, true]
);
graph_case!(
    disconnected_cycles,
    vec![vec![1], vec![0], vec![], vec![3]],
    256,
    vec![true, true, false, true]
);
graph_case!(
    cyclic_outgoing_tail_counts_toward_depth,
    vec![vec![1], vec![2], vec![1, 3], vec![4], vec![]],
    4,
    vec![true, true, true, false, false]
);
graph_case!(
    parallel_edges_do_not_invent_cycle,
    vec![vec![1, 1], vec![]],
    256,
    vec![false, false]
);

macro_rules! chain_case {
    ($name:ident, $length:expr, $rejected:expr) => {
        #[test]
        fn $name() {
            // Arrange: length counts edges; no recursive production walk is allowed.
            let edges = (0..=$length)
                .map(|v| if v < $length { vec![v + 1] } else { vec![] })
                .collect::<Vec<_>>();
            // Act.
            let actual = graph(&edges).excluded(256, &|| false).unwrap();
            // Assert: exactly vertices at least 256 edges from the end are rejected.
            let expected = (0..=$length).map(|v| v < $rejected).collect::<Vec<_>>();
            assert_eq!(actual, expected);
        }
    };
}
chain_case!(depth_255_is_eligible, 255, 0);
chain_case!(depth_256_is_excluded, 256, 1);
chain_case!(long_chain_uses_bounded_stack, 4096, 3841);

macro_rules! prefix_cycle_case {
    ($name:ident, $size:expr, $prefix_rejected:expr) => {
        #[test]
        fn $name() {
            // Arrange: p enters a simple cycle once; its terminal duplicate adds an edge.
            let mut edges = vec![vec![1]];
            edges.extend((1..=$size).map(|v| vec![if v == $size { 1 } else { v + 1 }]));
            // Act.
            let actual = graph(&edges).excluded(256, &|| false).unwrap();
            // Assert.
            assert_eq!(actual[0], $prefix_rejected);
            assert!(actual[1..].iter().all(|&rejected| rejected));
        }
    };
}
prefix_cycle_case!(prefix_into_254_cycle_is_eligible, 254, false);
prefix_cycle_case!(prefix_into_255_cycle_is_excluded, 255, true);

#[test]
fn branching_large_component_can_conservatively_exclude_prefix() {
    // Arrange: 255-node star SCC has short simple paths, but weight 255.
    let mut edges = vec![vec![1], (2..=255).collect()];
    edges.extend((2..=255).map(|_| vec![1]));
    let old = old_exclusions(&edges, 256);
    // Act.
    let actual = graph(&edges).excluded(256, &|| false).unwrap();
    // Assert: an explicit permitted extra exclusion, not a restored candidate.
    assert!(!old[0]);
    assert!(actual.iter().all(|&rejected| rejected));
}
