use std::collections::VecDeque;
use ahash::{AHashMap as HashMap, AHashSet as HashSet};

use reinda_core::AssetId;


pub(crate) struct IncludeGraph(HashMap<AssetId, NodeData>);

#[derive(Default)]
struct NodeData {
    includes: HashSet<AssetId>,
    included_by: HashSet<AssetId>,
}

impl IncludeGraph {
    pub(crate) fn new() -> Self {
        Self(HashMap::new())
    }

    pub(crate) fn add_include(&mut self, includer: AssetId, includee: AssetId) {
        self.0.entry(includer).or_default().includes.insert(includee);
        self.0.entry(includee).or_default().included_by.insert(includer);
    }

    /// Returns a topological sorting of this include graph.
    ///
    /// The first element of the returned list does not include any other asset.
    /// In general, includes can simply be resolved by iterating over the
    /// returned list forwards. If the graph is not a DAG, a vector containing
    /// one cycle is returned.
    pub(crate) fn topological_sort(mut self) -> Result<Vec<AssetId>, Vec<AssetId>> {
        // This is an implementation of Kahn's algorithm.

        let mut queue: VecDeque<_> = self.0.iter()
            .filter(|(_, data)| data.includes.is_empty())
            .map(|(id, _)| *id)
            .collect();

        let mut pos = 0;
        while let Some(&includer_id) = queue.get(pos) {
            pos += 1;
            while let Some(includee_id) = {
                // This is a strange workaround to make the compiler understand
                // the `Drain` iterator can be dropped before the loop body.
                let x = self.node_mut(includer_id).included_by.drain().next();
                x
            } {
                let includee = self.node_mut(includee_id);
                includee.includes.remove(&includer_id);
                if includee.includes.is_empty() {
                    queue.push_back(includee_id);
                }
            }
        }

        if queue.len() == self.0.len() {
            Ok(queue.into())
        } else {
            // For error reporting, we want to return a cycle here. It is not
            // super cheap, but as it only happens in case of an error, it's
            // fine.
            let (&start_id, _) = self.0.iter()
                .find(|(_, data)| !data.includes.is_empty())
                .expect("can't find node with edges, but there should be a cycle");

            let mut out = vec![start_id];
            let mut id = start_id;
            loop {
                // We can just follow one arbitrary edge as all edges now are
                // part of a cycle. However, it might not
                let next = *self.0[&id].includes.iter().next().unwrap();
                if let Some(pos) = out.iter().position(|&visited| visited == next) {
                    out.drain(..pos);
                    return Err(out);
                }

                out.push(next);
                id = next;
            }
        }
    }

    fn node_mut(&mut self, id: AssetId) -> &mut NodeData {
        self.0.get_mut(&id).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(edges: &[(u32, u32)]) -> IncludeGraph {
        let mut g = IncludeGraph::new();
        for &(from, to) in edges {
            g.add_include(AssetId(from), AssetId(to));
        }
        g
    }

    macro_rules! assert_topsort {
        (
            [$($from:literal includes $to:literal),* $(,)?]
            => $res:ident($( [$($id:literal),*] ),* $(,)?)
        ) => {
            let actual = graph(&[$( ($from, $to) ),*]).topological_sort();
            let valid = [
                $( $res(vec![$(AssetId($id)),*]) ),*,
            ];

            if !valid.contains(&actual) {
                panic!("`assert_topsort` failed: {:?} is not in valid solutions: {:#?}", actual, valid);
            }
        };
    }


    #[test]
    fn topological_sort_empty() {
        assert_topsort!([] => Ok([]));
    }

    #[test]
    fn topological_sort_dag() {
        assert_topsort!([0 includes 1] => Ok([1, 0]));
        assert_topsort!([1 includes 0] => Ok([0, 1]));

        assert_topsort!([1 includes 0, 2 includes 1] => Ok([0, 1, 2]));
        assert_topsort!([2 includes 9, 0 includes 2] => Ok([9, 2, 0]));

        assert_topsort!([0 includes 1, 0 includes 2] => Ok([1, 2, 0], [2, 1, 0]));
    }

    #[test]
    fn topological_sort_cycles() {
        assert_topsort!(
            [0 includes 1, 1 includes 2, 2 includes 0, 0 includes 4]
            => Err([0, 1, 2], [1, 2, 0], [2, 0, 1])
        );

        assert_topsort!(
            [
                0 includes 1, 1 includes 2, 2 includes 0,
                1 includes 3, 3 includes 2,
                3 includes 4, 4 includes 5,
            ]
            => Err(
                [0, 1, 2], [1, 2, 0], [2, 0, 1],
                [3, 1, 2], [1, 2, 3], [2, 3, 1],
                [0, 1, 3, 2], [1, 3, 2, 0], [3, 2, 0, 1], [2, 0, 1, 3],
            )
        );
    }
}
