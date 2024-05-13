use std::{collections::VecDeque, mem};
use ahash::{HashMap, HashMapExt, HashSet};


#[derive(Debug)]
pub(crate) struct DepGraph<'a>(HashMap<&'a str, NodeData<'a>>);

#[derive(Default)]
#[derive(Debug)]
struct NodeData<'a> {
    /// List of assets this asset depends on.
    dependencies: HashSet<&'a str>,

    /// Set of assets that are dependant on this asset.
    rev_dependencies: HashSet<&'a str>,
}

impl<'a> DepGraph<'a> {
    pub(crate) fn new() -> Self {
        Self(HashMap::new())
    }

    /// Explicitly adds an asset to the graph. This makes sure this asset is
    /// included in the topological sort. It is as if it would register an
    /// external dependency on `id`. Assets are automatically created when you
    /// call `add_dependency`.
    pub(crate) fn add_asset(&mut self, id: &'a str) {
        self.0.entry(id).or_default();
    }

    /// Adds one edge to this graph: `depender` depends on `dependee`.
    pub(crate) fn add_dependency(&mut self, depender: &'a str, dependee: &'a str) {
        self.0.entry(depender).or_default().dependencies.insert(dependee);
        self.0.entry(dependee).or_default().rev_dependencies.insert(depender);
    }

    /// Returns a topological sorting of this dependency graph.
    ///
    /// The first element of the returned list does not have any dependencies.
    /// In general, dependencies can simply be resolved by iterating over the
    /// returned list forwards. If the graph is not a DAG, a vector containing
    /// one cycle is returned.
    pub(crate) fn topological_sort(mut self) -> Result<Vec<&'a str>, Vec<&'a str>> {
        // This is an implementation of Kahn's algorithm.

        let mut queue: VecDeque<_> = self.0.iter()
            .filter(|(_, data)| data.dependencies.is_empty())
            .map(|(id, _)| *id)
            .collect();

        let mut pos = 0;
        while let Some(&dependee_id) = queue.get(pos) {
            pos += 1;
            let rev_deps = mem::take(&mut self.0.get_mut(&dependee_id).unwrap().rev_dependencies).into_iter();

            for depender_id in rev_deps {
                let depender = self.0.get_mut(&depender_id).unwrap();
                let was_removed = depender.dependencies.remove(&dependee_id);
                debug_assert!(was_removed);

                // If we just removed the last dependency of `depender`, then it
                // is now ready to be processed.
                if depender.dependencies.is_empty() {
                    queue.push_back(depender_id);
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
                .find(|(_, data)| !data.dependencies.is_empty())
                .expect("can't find node with edges, but there should be a cycle");

            let mut out = vec![start_id];
            let mut id = start_id;
            loop {
                // We can just follow one arbitrary edge as all edges now are
                // part of a cycle. However, it might not
                let next = *self.0[&id].dependencies.iter().next().unwrap();
                if let Some(pos) = out.iter().position(|&visited| visited == next) {
                    out.drain(..pos);
                    return Err(out);
                }

                out.push(next);
                id = next;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph<'a>(edges: &[(&'a str, &'a str)]) -> DepGraph<'a> {
        let mut g = DepGraph::new();
        for &(from, to) in edges {
            g.add_dependency(from, to);
        }
        g
    }

    macro_rules! assert_topsort {
        (
            [$($from:literal <- $to:literal),* $(,)?]
            => $res:ident($( [$($id:literal),*] ),* $(,)?)
        ) => {
            let actual = graph(&[$( ($from, $to) ),*]).topological_sort();
            let valid = [
                $( $res(vec![$($id),*]) ),*,
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
        assert_topsort!(["a" <- "b"] => Ok(["b", "a"]));
        assert_topsort!(["b" <- "a"] => Ok(["a", "b"]));

        assert_topsort!(["b" <- "a", "c" <- "b"] => Ok(["a", "b", "c"]));
        assert_topsort!(["c" <- "f", "a" <- "c"] => Ok(["f", "c", "a"]));

        assert_topsort!(["a" <- "b", "a" <- "c"] => Ok(["b", "c", "a"], ["c", "b", "a"]));
    }

    #[test]
    fn topological_sort_cycles() {
        assert_topsort!(
            ["a" <- "b", "b" <- "c", "c" <- "a", "a" <- "e"]
            => Err(["a", "b", "c"], ["b", "c", "a"], ["c", "a", "b"])
        );

        assert_topsort!(
            [
                "a" <- "b", "b" <- "c", "c" <- "a",
                "b" <- "d", "d" <- "c",
                "d" <- "e", "e" <- "f",
            ]
            => Err(
                ["a", "b", "c"], ["b", "c", "a"], ["c", "a", "b"],
                ["d", "b", "c"], ["b", "c", "d"], ["c", "d", "b"],
                ["a", "b", "d", "c"], ["b", "d", "c", "a"],
                ["d", "c", "a", "b"], ["c", "a", "b", "d"],
            )
        );
    }
}
