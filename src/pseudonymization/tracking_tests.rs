use super::*;
use mdoc_editor::HistoryChange;
fn plan(range: Range<usize>, before: Arc<str>, after: Arc<str>) -> ReplacementPlan {
    ReplacementPlan {
        range,
        before,
        after,
        category: Category::Person,
        identity: 0,
    }
}
fn edit(
    tracking: &mut Tracking,
    revision: u64,
    before: u64,
    after: u64,
    range: Range<usize>,
    new_len: usize,
    retained: &[u64],
) {
    tracking.on_transaction(&EditorTransaction {
        revision,
        changes: vec![HistoryChange {
            before,
            after,
            edits: vec![SourceEdit { range, new_len }],
        }],
        retained: retained.to_vec(),
    });
}
fn travel(tracking: &mut Tracking, revision: u64, before: u64, after: u64, retained: &[u64]) {
    tracking.on_transaction(&EditorTransaction {
        revision,
        changes: vec![HistoryChange {
            before,
            after,
            edits: vec![],
        }],
        retained: retained.to_vec(),
    });
}
fn replace(
    tracking: &mut Tracking,
    before: &str,
    after: &str,
    range: Range<usize>,
    revision: u64,
    from: u64,
    to: u64,
) {
    let added = tracking.prepare(&[plan(range.clone(), before.into(), after.into())]);
    edit(tracking, revision, from, to, range, after.len(), &[0, to]);
    tracking.commit(to, added);
}
#[test]
fn coalesced_typing_invalidation_undo_redo_and_branching_preserve_occurrence_identity() {
    let mut t = Tracking::default();
    replace(&mut t, "Анна", "PERSON", 0..8, 1, 0, 1);
    let id = t.applied[0].id;
    edit(&mut t, 2, 1, 2, 0..0, 1, &[0, 1, 2]);
    edit(&mut t, 3, 2, 3, 1..1, 1, &[0, 1, 3]);
    assert_eq!(t.get(id).unwrap().range, 2..8);
    travel(&mut t, 4, 3, 1, &[0, 1, 3]);
    assert_eq!(t.get(id).unwrap().range, 0..6);
    travel(&mut t, 5, 1, 3, &[0, 1, 3]);
    assert_eq!(t.get(id).unwrap().range, 2..8);
    edit(&mut t, 6, 3, 4, 3..3, 1, &[0, 1, 3, 4]);
    assert!(t.get(id).is_none());
    travel(&mut t, 7, 4, 3, &[0, 1, 3, 4]);
    assert_eq!(t.get(id).unwrap().step.before.as_ref(), "Анна");
    edit(&mut t, 8, 3, 5, 8..8, 1, &[0, 1, 3, 5]);
    assert_eq!(t.get(id).unwrap().range, 2..8);
    assert!(!t.journal.contains_key(&4));
}
#[test]
fn restoration_keep_and_chained_predecessors_travel_with_text_history() {
    let mut t = Tracking::default();
    replace(&mut t, "Anna", "PERSON_1", 0..4, 1, 0, 1);
    let id = t.applied[0].id;
    replace(&mut t, "PERSON_1", "PERSON", 0..8, 2, 1, 2);
    let plan = t.restore_plan("PERSON", id, false).unwrap();
    assert_eq!(plan, vec![(0..6, "PERSON_1".into())]);
    let restored = t.prepare_restore(&plan);
    edit(&mut t, 3, 2, 3, 0..6, 8, &[0, 1, 2, 3]);
    t.commit_restore(3, restored);
    assert_eq!(t.get(id).unwrap().step.before.as_ref(), "Anna");
    assert_eq!(t.exclusions[0].original.as_ref(), "PERSON_1");
    travel(&mut t, 4, 3, 2, &[0, 1, 2, 3]);
    assert!(t.exclusions.is_empty());
    assert_eq!(t.get(id).unwrap().step.before.as_ref(), "PERSON_1");
    travel(&mut t, 5, 2, 3, &[0, 1, 2, 3]);
    assert_eq!(t.exclusions.len(), 1);
    assert_eq!(t.get(id).unwrap().step.before.as_ref(), "Anna");
}
#[test]
fn same_marker_is_not_same_original_and_pasted_tokens_have_no_provenance() {
    let mut t = Tracking::default();
    let plans = vec![
        plan(0..4, "Anna".into(), "PERSON".into()),
        plan(5..8, "Bob".into(), "PERSON".into()),
        plan(9..13, "Anna".into(), "PERSON".into()),
    ];
    let added = t.prepare(&plans);
    t.on_transaction(&EditorTransaction {
        revision: 1,
        changes: vec![HistoryChange {
            before: 0,
            after: 1,
            edits: plans
                .iter()
                .map(|p| SourceEdit {
                    range: p.range.clone(),
                    new_len: p.after.len(),
                })
                .collect(),
        }],
        retained: vec![0, 1],
    });
    t.commit(1, added);
    let id = t.applied[0].id;
    assert_eq!(
        t.restore_plan("PERSON PERSON PERSON", id, true).unwrap(),
        vec![(0..6, "Anna".into()), (14..20, "Anna".into())]
    );
    assert!(Arc::ptr_eq(&t.applied[0].step, &t.applied[2].step));
    edit(&mut t, 2, 1, 2, 20..20, 7, &[0, 1, 2]);
    assert_eq!(t.applied.len(), 3);
    assert!(t.at(&(21..27)).is_none());
}
#[test]
fn pruned_history_releases_invalidated_original_but_retains_active_provenance() {
    let mut t = Tracking::default();
    replace(&mut t, "Anna", "PERSON", 0..4, 1, 0, 1);
    let weak = Arc::downgrade(&t.applied[0].step);
    edit(&mut t, 2, 1, 2, 0..6, 0, &[0, 1, 2]);
    assert!(weak.upgrade().is_some());
    edit(&mut t, 3, 2, 3, 0..0, 1, &[2, 3]);
    assert!(weak.upgrade().is_none());
    replace(&mut t, "Bob", "PERSON", 1..4, 4, 3, 4);
    let weak = Arc::downgrade(&t.applied[0].step);
    edit(&mut t, 5, 4, 5, 0..0, 1, &[4, 5]);
    assert!(weak.upgrade().is_some());
}

#[test]
#[ignore = "20,000 occurrence metadata/history stress; run on an idle machine"]
fn metadata_history_stress_retains_shared_originals_and_prunes_reversible_deltas() {
    let mut tracking = Tracking::default();
    let original: Arc<str> = "Анна".into();
    let marker: Arc<str> = "PERSON".into();
    let original_weak = Arc::downgrade(&original);
    let plans: Vec<_> = (0..20_000)
        .map(|n| plan(14 * n..14 * n + 8, original.clone(), marker.clone()))
        .collect();
    let added = tracking.prepare(&plans);
    let weak = Arc::downgrade(&added[0].step);
    assert!(added.iter().all(|o| Arc::ptr_eq(&o.step, &added[0].step)));
    tracking.on_transaction(&EditorTransaction {
        revision: 1,
        changes: vec![HistoryChange {
            before: 0,
            after: 1,
            edits: plans
                .iter()
                .map(|p| SourceEdit {
                    range: p.range.clone(),
                    new_len: p.after.len(),
                })
                .collect(),
        }],
        retained: vec![0, 1],
    });
    tracking.commit(1, added);
    drop(plans);
    drop(original);
    for id in 2u64..=301 {
        let retained: Vec<_> = (id.saturating_sub(255)..=id).collect();
        edit(
            &mut tracking,
            id,
            id - 1,
            id,
            (2_057_152 + (id as usize - 2))..(2_057_152 + (id as usize - 2)),
            1,
            &retained,
        );
    }
    assert_eq!(tracking.applied.len(), 20_000);
    assert!(tracking.journal.len() <= 255);
    assert!(weak.upgrade().is_some());
    let occurrence_bytes = tracking.applied.capacity() * std::mem::size_of::<Applied>();
    let delta_bytes: usize = tracking
        .journal
        .values()
        .map(|d| {
            std::mem::size_of::<Delta>()
                + d.edits.capacity() * std::mem::size_of::<SourceEdit>()
                + (d.removed.capacity() + d.added.capacity()) * std::mem::size_of::<Applied>()
                + (d.removed_keeps.capacity() + d.added_keeps.capacity())
                    * std::mem::size_of::<Exclusion>()
        })
        .sum();
    eprintln!(
        "PII_STORAGE occurrences=20000 shared_steps=1 retained_deltas={} occurrence_capacity_bytes={occurrence_bytes} delta_capacity_bytes={delta_bytes} lookup_capacity={} (excludes hash buckets/allocator overhead)",
        tracking.journal.len(),
        tracking.lookup.capacity()
    );
    edit(
        &mut tracking,
        302,
        301,
        302,
        0..2_057_452,
        0,
        &(47..=302).collect::<Vec<_>>(),
    );
    assert!(tracking.applied.is_empty());
    assert!(
        weak.upgrade().is_some(),
        "undo history still owns originals"
    );
    for id in 303..=559 {
        edit(
            &mut tracking,
            id,
            id - 1,
            id,
            0..0,
            1,
            &(id - 255..=id).collect::<Vec<_>>(),
        );
    }
    assert!(
        weak.upgrade().is_none(),
        "pruned history releases original steps"
    );
    assert!(original_weak.upgrade().is_none());
}

#[test]
fn coalesced_adjacent_typing_and_deletion_compact_without_losing_invalidated_fields() {
    let mut t = Tracking::default();
    replace(&mut t, "Anna", "PERSON", 10..14, 1, 0, 1);
    let id = t.applied[0].id;
    for after in 2..=1002 {
        edit(&mut t, after, after - 1, after, 0..0, 1, &[0, 1, after]);
    }
    assert_eq!(
        t.journal.len(),
        2,
        "one batch plus one coalesced typing delta"
    );
    assert_eq!(t.get(id).unwrap().range, 1011..1017);
    travel(&mut t, 1003, 1002, 1, &[0, 1, 1002]);
    assert_eq!(t.get(id).unwrap().range, 10..16);
    travel(&mut t, 1004, 1, 1002, &[0, 1, 1002]);
    assert_eq!(t.get(id).unwrap().range, 1011..1017);
    // A coalesced deletion first consumes the gap, then invalidates the marker.
    edit(&mut t, 1005, 1002, 1005, 1008..1011, 0, &[0, 1, 1002, 1005]);
    edit(&mut t, 1006, 1005, 1006, 1008..1010, 0, &[0, 1, 1002, 1006]);
    assert!(t.get(id).is_none());
    assert!(!t.journal.contains_key(&1005));
    travel(&mut t, 1007, 1006, 1002, &[0, 1, 1002, 1006]);
    assert_eq!(t.get(id).unwrap().range, 1011..1017);
    travel(&mut t, 1008, 1002, 1006, &[0, 1, 1002, 1006]);
    assert!(t.get(id).is_none());
}
