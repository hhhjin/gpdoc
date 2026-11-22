//! Cases from prosemirror-transform 1.12.6, test/test-mapping.ts.
use gpdoc_core::transform::{Mapping, StepMap};

fn mapping(ranges: &[[usize; 3]]) -> Mapping {
    Mapping::new(ranges.iter().map(|&r| StepMap::new(vec![r])).collect())
}

fn check(map: &Mapping, cases: &[(usize, usize, i8, bool)]) {
    let inverse = map.invert();
    for &(from, to, bias, lossy) in cases {
        assert_eq!(map.map(from, bias), to);
        if !lossy {
            assert_eq!(inverse.map(to, bias), from);
        }
    }
}

#[test]
fn insertion_deletion_and_replacement() {
    check(
        &mapping(&[[2, 0, 4]]),
        &[
            (0, 0, 1, false),
            (2, 6, 1, false),
            (2, 2, -1, false),
            (3, 7, 1, false),
        ],
    );
    check(
        &mapping(&[[2, 4, 0]]),
        &[
            (0, 0, 1, false),
            (2, 2, -1, false),
            (3, 2, 1, true),
            (6, 2, 1, false),
            (6, 2, -1, true),
            (7, 3, 1, false),
        ],
    );
    check(
        &mapping(&[[2, 4, 4]]),
        &[
            (0, 0, 1, false),
            (2, 2, 1, false),
            (4, 6, 1, true),
            (4, 2, -1, true),
            (6, 6, -1, false),
            (8, 8, 1, false),
        ],
    );
}

#[test]
fn mirror_pairs_recover_interior_positions() {
    let mut map = mapping(&[[2, 4, 0]]);
    map.append_map(StepMap::new(vec![[2, 0, 4]]), Some(0));
    check(
        &map,
        &[
            (0, 0, 1, false),
            (2, 2, 1, false),
            (4, 4, 1, false),
            (6, 6, 1, false),
            (7, 7, 1, false),
        ],
    );
    let mut map = mapping(&[[2, 0, 4]]);
    map.append_map(StepMap::new(vec![[2, 4, 0]]), Some(0));
    check(
        &map,
        &[(0, 0, 1, false), (2, 2, 1, false), (3, 3, 1, false)],
    );
    let mut map = mapping(&[[2, 4, 0], [1, 0, 1]]);
    map.append_map(StepMap::new(vec![[3, 0, 4]]), Some(0));
    check(
        &map,
        &[
            (0, 0, 1, false),
            (1, 2, 1, false),
            (4, 5, 1, false),
            (6, 7, 1, false),
            (7, 8, 1, false),
        ],
    );
    assert!(!map.map_result(4, 1).deleted());
    assert!(map.slice(0, 2).map_result(4, 1).deleted());
}

#[test]
fn deletions_report_side_and_across_flags() {
    let check = |maps: &[[usize; 3]], side: i8, expected: &str| {
        let result = mapping(maps).map_result(2, side);
        let flags: String = [
            (result.deleted(), 'd'),
            (result.deleted_before(), 'b'),
            (result.deleted_after(), 'a'),
            (result.deleted_across(), 'x'),
        ]
        .into_iter()
        .filter_map(|(set, c)| set.then_some(c))
        .collect();
        assert_eq!(flags, expected, "{maps:?}, side {side}");
    };
    check(&[[0, 2, 0]], -1, "db");
    check(&[[0, 2, 0]], 1, "b");
    check(&[[0, 2, 2]], -1, "db");
    check(&[[0, 1, 0], [0, 1, 0]], -1, "db");
    check(&[[0, 1, 0]], -1, "");
    check(&[[2, 2, 0]], -1, "a");
    check(&[[2, 2, 0]], 1, "da");
    check(&[[2, 2, 2]], 1, "da");
    check(&[[2, 1, 0], [2, 1, 0]], 1, "da");
    check(&[[3, 2, 0]], -1, "");
    check(&[[0, 4, 0]], -1, "dbax");
    check(&[[0, 4, 0]], 1, "dbax");
    check(&[[0, 1, 0], [4, 1, 0], [0, 3, 0]], 1, "dbax");
    check(&[[4, 1, 0], [0, 1, 0]], -1, "");
    check(&[[2, 1, 0], [0, 2, 0]], -1, "dba");
    check(&[[2, 1, 0], [0, 1, 0]], -1, "a");
    check(&[[3, 1, 0], [0, 2, 0]], -1, "db");
    check(&[[2, 0, 2]], -1, "");
    check(&[[2, 0, 2]], 1, "");
}
