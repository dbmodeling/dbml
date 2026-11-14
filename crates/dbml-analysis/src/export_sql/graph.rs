//! Foreign-key dependency graph and topological ordering for DDL emission.

use schema::{Endpoint, Ref, RefKind, Schema, Table};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Returns `(child, parent)` endpoints for a non-M2M relation.
pub(super) fn fk_child_parent(r: &Ref) -> (&Endpoint, &Endpoint) {
    if r.kind == RefKind::OneToMany {
        (&r.to, &r.from)
    } else {
        (&r.from, &r.to)
    }
}

/// Returns the referencing (child) endpoint of a relation.
pub(super) fn fk_child_endpoint(r: &Ref) -> &Endpoint {
    fk_child_parent(r).0
}

/// Orders tables so parent tables precede child tables, returning any cycle/self refs to defer.
pub(super) fn topological_order(schema: &Schema) -> (Vec<&Table>, Vec<&Ref>) {
    let mut table_map: BTreeMap<&str, &Table> = BTreeMap::new();
    let mut table_idx: HashMap<&str, usize> = HashMap::with_capacity(schema.tables.len());
    let mut indegree: BTreeMap<&str, usize> = BTreeMap::new();
    let mut dependents: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();

    for (idx, t) in schema.tables.iter().enumerate() {
        table_map.insert(t.name.as_str(), t);
        table_idx.insert(t.name.as_str(), idx);
        indegree.insert(t.name.as_str(), 0);
        dependents.entry(t.name.as_str()).or_default();
    }

    let mut edges: BTreeSet<(&str, &str)> = BTreeSet::new();
    for r in &schema.refs {
        if r.kind == RefKind::ManyToMany {
            continue;
        }
        let (child_ep, parent_ep) = fk_child_parent(r);
        let child = child_ep.table.as_str();
        let parent = parent_ep.table.as_str();
        if child != parent
            && table_map.contains_key(child)
            && table_map.contains_key(parent)
            && edges.insert((parent, child))
        {
            dependents.entry(parent).or_default().insert(child);
            if let Some(deg) = indegree.get_mut(child) {
                *deg += 1;
            }
        }
    }

    let mut ready: BTreeSet<(usize, &str)> = indegree
        .iter()
        .filter(|&(_, &deg)| deg == 0)
        .map(|(&name, _)| (table_idx.get(name).copied().unwrap_or(usize::MAX), name))
        .collect();
    let mut ordered: Vec<&Table> = Vec::with_capacity(schema.tables.len());
    let mut rank: HashMap<&str, usize> = HashMap::with_capacity(schema.tables.len());

    while let Some(&(idx, name)) = ready.iter().next() {
        ready.remove(&(idx, name));
        if let Some(&t) = table_map.get(name) {
            rank.insert(name, ordered.len());
            ordered.push(t);
        }
        if let Some(children) = dependents.get(name) {
            for &child in children {
                if let Some(deg) = indegree.get_mut(child) {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        let cidx = table_idx.get(child).copied().unwrap_or(usize::MAX);
                        ready.insert((cidx, child));
                    }
                }
            }
        }
    }

    // Append any remaining tables involved in FK cycles in deterministic order.
    for t in &schema.tables {
        if !rank.contains_key(t.name.as_str()) {
            rank.insert(t.name.as_str(), ordered.len());
            ordered.push(t);
        }
    }

    let deferred_refs = schema
        .refs
        .iter()
        .filter(|r| {
            if r.kind == RefKind::ManyToMany {
                return false;
            }
            let (child_ep, parent_ep) = fk_child_parent(r);
            let child = child_ep.table.as_str();
            let parent = parent_ep.table.as_str();
            if child == parent {
                return true;
            }
            match (rank.get(parent), rank.get(child)) {
                (Some(&pr), Some(&cr)) => pr >= cr,
                _ => true,
            }
        })
        .collect();

    (ordered, deferred_refs)
}
